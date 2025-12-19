# Copyright 2025 DataStax Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
# in compliance with the License. You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software distributed under the License
# is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
# or implied. See the License for the specific language governing permissions and limitations under
# the License.

"""HTTP client for Stepflow servers."""

from pathlib import Path
from typing import Any

import httpx
import yaml

from stepflow import (
    ComponentInfo,
    Diagnostic,
    FlowError,
    FlowResult,
    FlowResultStatus,
    ValidationResult,
)


class StepflowClientError(Exception):
    """Error from the Stepflow client."""

    def __init__(self, message: str, status_code: int | None = None, details: dict | None = None):
        super().__init__(message)
        self.status_code = status_code
        self.details = details


class StepflowClient:
    """HTTP client for remote Stepflow servers.

    Implements the StepflowExecutor protocol for interchangeable usage
    with StepflowRuntime.

    Example:
        ```python
        client = StepflowClient("http://localhost:7837")

        # Run a workflow and wait for result
        result = await client.run("workflow.yaml", {"x": 1, "y": 2})
        if result.is_success:
            print(f"Output: {result.output}")

        # Submit and get result later
        run_id = await client.submit("workflow.yaml", {"x": 1})
        result = await client.get_result(run_id)
        ```
    """

    def __init__(
        self,
        url: str = "http://localhost:7837",
        *,
        timeout: float = 300.0,
        headers: dict[str, str] | None = None,
    ):
        """Initialize the Stepflow client.

        Args:
            url: Base URL of the Stepflow server (e.g., "http://localhost:7837")
            timeout: Request timeout in seconds (default: 300s for long workflows)
            headers: Optional additional headers to include in requests
        """
        self._url = url.rstrip("/")
        self._timeout = timeout
        self._headers = headers or {}
        self._client: httpx.AsyncClient | None = None

    @property
    def url(self) -> str:
        """Base URL of the Stepflow server."""
        return self._url

    async def _get_client(self) -> httpx.AsyncClient:
        """Get or create the HTTP client."""
        if self._client is None or self._client.is_closed:
            self._client = httpx.AsyncClient(
                base_url=self._url,
                timeout=self._timeout,
                headers=self._headers,
            )
        return self._client

    async def close(self) -> None:
        """Close the HTTP client."""
        if self._client is not None and not self._client.is_closed:
            await self._client.aclose()
            self._client = None

    async def __aenter__(self) -> "StepflowClient":
        """Async context manager entry."""
        return self

    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        """Async context manager exit."""
        await self.close()

    def _load_flow(self, flow: str | Path | dict[str, Any]) -> dict[str, Any]:
        """Load a flow from file path or return as-is if already a dict."""
        if isinstance(flow, dict):
            return flow
        path = Path(flow)
        if not path.exists():
            raise FileNotFoundError(f"Workflow file not found: {path}")
        with open(path) as f:
            return yaml.safe_load(f)

    async def _store_flow(self, flow: dict[str, Any]) -> tuple[str | None, ValidationResult]:
        """Store a flow and return flow_id and validation result."""
        client = await self._get_client()
        response = await client.post(
            "/api/v1/flows",
            json={"flow": flow},
        )

        if response.status_code >= 400:
            self._handle_error_response(response, "storing workflow")

        data = response.json()

        # Parse validation diagnostics
        # Note: AnalysisResult is flattened into the response, so diagnostics is at top level
        diagnostics = []
        diag_container = data.get("diagnostics", {})
        for diag in diag_container.get("diagnostics", []):
            # Skip diagnostics marked as ignore
            if diag.get("ignore", False):
                continue
            diagnostics.append(
                Diagnostic(
                    level=diag.get("level", "error"),
                    message=diag.get("text", ""),  # Use 'text' field for human-readable message
                    location=diag.get("path"),  # Use 'path' field for location
                )
            )

        validation = ValidationResult(
            valid=data.get("flowId") is not None,
            diagnostics=diagnostics,
        )

        return data.get("flowId"), validation

    def _handle_error_response(self, response: httpx.Response, context: str) -> None:
        """Handle an error response from the server."""
        try:
            data = response.json()
            message = data.get("message", response.text)
            details = data
        except Exception:
            message = response.text or f"HTTP {response.status_code}"
            details = None

        raise StepflowClientError(
            f"Error {context}: {message}",
            status_code=response.status_code,
            details=details,
        )

    def _parse_flow_result(self, data: dict[str, Any]) -> FlowResult:
        """Parse a flow result from the API response."""
        result = data.get("result")

        # If no result key, use the whole data
        if result is None:
            result = data

        # Handle case where result is not a dict (shouldn't happen but be defensive)
        if not isinstance(result, dict):
            return FlowResult(status=FlowResultStatus.SUCCESS, output=result)

        # Check for different result formats
        if "Success" in result:
            return FlowResult(
                status=FlowResultStatus.SUCCESS,
                output=result["Success"],
            )
        elif "Failed" in result:
            error_data = result["Failed"]
            return FlowResult(
                status=FlowResultStatus.FAILED,
                error=FlowError(
                    code=error_data.get("code", 500),
                    message=error_data.get("message", "Unknown error"),
                    details=error_data.get("details"),
                ),
            )
        elif "Skipped" in result:
            return FlowResult(
                status=FlowResultStatus.SKIPPED,
                skipped_reason=result["Skipped"].get("reason"),
            )
        # Check for outcome-based format (from run execution)
        elif "outcome" in result:
            outcome = result["outcome"]
            if outcome == "success":
                # Output is in the 'result' field for success
                return FlowResult(
                    status=FlowResultStatus.SUCCESS,
                    output=result.get("result", result.get("output", result)),
                )
            elif outcome == "failed":
                error_data = result.get("error", {})
                return FlowResult(
                    status=FlowResultStatus.FAILED,
                    error=FlowError(
                        code=error_data.get("code", 500),
                        message=error_data.get("message", "Execution failed"),
                        details=error_data.get("data"),
                    ),
                )
            elif outcome == "skipped":
                return FlowResult(
                    status=FlowResultStatus.SKIPPED,
                    skipped_reason=result.get("reason"),
                )
        # Default: treat as direct output
        return FlowResult(status=FlowResultStatus.SUCCESS, output=result)

    async def run(
        self,
        flow: str | Path | dict[str, Any],
        input: dict[str, Any],
        overrides: dict[str, Any] | None = None,
    ) -> FlowResult:
        """Run a workflow and wait for the result.

        Args:
            flow: Path to workflow file, or workflow dict
            input: Input data for the workflow
            overrides: Optional step overrides

        Returns:
            FlowResult with success/failure status and output

        Raises:
            StepflowClientError: If the request fails
            FileNotFoundError: If the workflow file doesn't exist
        """
        flow_dict = self._load_flow(flow)

        # Store the flow first
        flow_id, validation = await self._store_flow(flow_dict)
        if not validation.valid or flow_id is None:
            # Return validation errors as a failed result
            error_msgs = [d.message for d in validation.errors]
            return FlowResult.failed(
                code=400,
                message="Workflow validation failed",
                details={"errors": error_msgs, "diagnostics": validation.diagnostics},
            )

        # Create and execute the run
        client = await self._get_client()
        request_body: dict[str, Any] = {
            "flowId": flow_id,
            "input": input,
            "debug": False,
        }
        if overrides:
            request_body["overrides"] = overrides

        response = await client.post("/api/v1/runs", json=request_body)

        if response.status_code >= 400:
            self._handle_error_response(response, "executing workflow")

        data = response.json()
        return self._parse_flow_result(data)

    async def submit(
        self,
        flow: str | Path | dict[str, Any],
        input: dict[str, Any],
        overrides: dict[str, Any] | None = None,
    ) -> str:
        """Submit a workflow for execution and return immediately.

        Args:
            flow: Path to workflow file, or workflow dict
            input: Input data for the workflow
            overrides: Optional step overrides

        Returns:
            Run ID for tracking the execution

        Raises:
            StepflowClientError: If the request fails
            FileNotFoundError: If the workflow file doesn't exist
        """
        flow_dict = self._load_flow(flow)

        # Store the flow first
        flow_id, validation = await self._store_flow(flow_dict)
        if not validation.valid or flow_id is None:
            error_msgs = [d.message for d in validation.errors]
            raise StepflowClientError(
                "Workflow validation failed",
                status_code=400,
                details={"errors": error_msgs},
            )

        # Create the run (would need async execution support on server)
        client = await self._get_client()
        request_body: dict[str, Any] = {
            "flowId": flow_id,
            "input": input,
            "debug": False,
        }
        if overrides:
            request_body["overrides"] = overrides

        response = await client.post("/api/v1/runs", json=request_body)

        if response.status_code >= 400:
            self._handle_error_response(response, "submitting workflow")

        data = response.json()
        return data.get("runId", "")

    async def get_result(self, run_id: str) -> FlowResult:
        """Get the result of a submitted workflow run.

        Args:
            run_id: The run ID returned from submit()

        Returns:
            FlowResult with the execution outcome

        Raises:
            StepflowClientError: If the request fails
        """
        client = await self._get_client()
        response = await client.get(f"/api/v1/runs/{run_id}")

        if response.status_code >= 400:
            self._handle_error_response(response, f"getting run {run_id}")

        data = response.json()
        return self._parse_flow_result(data)

    async def validate(
        self,
        flow: str | Path | dict[str, Any],
    ) -> ValidationResult:
        """Validate a workflow without executing it.

        Args:
            flow: Path to workflow file, or workflow dict

        Returns:
            ValidationResult with diagnostics

        Raises:
            FileNotFoundError: If the workflow file doesn't exist
        """
        flow_dict = self._load_flow(flow)
        _, validation = await self._store_flow(flow_dict)
        return validation

    async def list_components(self) -> list[ComponentInfo]:
        """List all available components.

        Returns:
            List of ComponentInfo with component details

        Raises:
            StepflowClientError: If the request fails
        """
        client = await self._get_client()
        response = await client.get("/api/v1/components")

        if response.status_code >= 400:
            self._handle_error_response(response, "listing components")

        data = response.json()
        components = []
        for comp in data.get("components", []):
            components.append(
                ComponentInfo(
                    path=comp.get("component", ""),
                    description=comp.get("description"),
                    input_schema=comp.get("input_schema"),
                    output_schema=comp.get("output_schema"),
                )
            )
        return components

    async def health(self) -> dict[str, Any]:
        """Check the health of the Stepflow server.

        Returns:
            Health status dict with status, timestamp, version

        Raises:
            StepflowClientError: If the request fails
        """
        client = await self._get_client()
        response = await client.get("/api/v1/health")

        if response.status_code >= 400:
            self._handle_error_response(response, "health check")

        return response.json()

    async def submit_batch(
        self,
        flow: str | Path | dict[str, Any],
        inputs: list[dict[str, Any]],
        max_concurrent: int | None = None,
        overrides: dict[str, Any] | None = None,
    ) -> str:
        """Submit a batch of workflow runs.

        Args:
            flow: Path to workflow file, or workflow dict
            inputs: List of input dicts for each workflow run
            max_concurrent: Maximum concurrent executions (default: all)
            overrides: Optional step overrides (applied to all runs)

        Returns:
            Batch ID for tracking

        Raises:
            StepflowClientError: If the request fails
        """
        flow_dict = self._load_flow(flow)

        # Store the flow first
        flow_id, validation = await self._store_flow(flow_dict)
        if not validation.valid or flow_id is None:
            error_msgs = [d.message for d in validation.errors]
            raise StepflowClientError(
                "Workflow validation failed",
                status_code=400,
                details={"errors": error_msgs},
            )

        client = await self._get_client()
        request_body: dict[str, Any] = {
            "flowId": flow_id,
            "inputs": inputs,
        }
        if max_concurrent is not None:
            request_body["maxConcurrent"] = max_concurrent
        if overrides:
            request_body["overrides"] = overrides

        response = await client.post("/api/v1/batches", json=request_body)

        if response.status_code >= 400:
            self._handle_error_response(response, "submitting batch")

        data = response.json()
        return data.get("batchId", "")

    async def get_batch(
        self,
        batch_id: str,
        include_results: bool = True,
    ) -> tuple[dict[str, Any], list[FlowResult] | None]:
        """Get batch status and optionally results.

        Args:
            batch_id: The batch ID
            include_results: Whether to include individual results

        Returns:
            Tuple of (batch_details, list of FlowResults or None)

        Raises:
            StepflowClientError: If the request fails
        """
        client = await self._get_client()

        # Get batch details
        response = await client.get(f"/api/v1/batches/{batch_id}")
        if response.status_code >= 400:
            self._handle_error_response(response, f"getting batch {batch_id}")

        details = response.json()

        results = None
        if include_results:
            # Get batch outputs from separate endpoint
            response = await client.get(f"/api/v1/batches/{batch_id}/outputs")
            if response.status_code >= 400:
                self._handle_error_response(response, f"getting batch outputs {batch_id}")

            outputs_data = response.json()
            results = []
            for output_info in outputs_data.get("outputs", []):
                if "result" in output_info and output_info["result"] is not None:
                    results.append(self._parse_flow_result({"result": output_info["result"]}))

        return details, results
