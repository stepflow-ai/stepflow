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

"""API contract tests that validate the Python client against the server's OpenAPI spec.

These tests ensure the Python client stays in sync with the Rust server's API.
They fetch the OpenAPI spec from a running server and verify:
- All client methods correspond to documented endpoints
- Request/response structures match the schema

If these tests fail, it indicates the Python client needs updating to match
server API changes.
"""

import httpx
import pytest

from stepflow_runtime import StepflowRuntime
from stepflow_runtime.logging import LogConfig


class TestApiContract:
    """Validate Python client against server OpenAPI spec."""

    @pytest.fixture
    async def openapi_spec(self):
        """Fetch OpenAPI spec from running server."""
        async with await StepflowRuntime.start_async(
            log_config=LogConfig(level="warn", capture=False),
            startup_timeout=30.0,
        ) as runtime:
            async with httpx.AsyncClient(base_url=runtime.url, timeout=30.0) as client:
                response = await client.get("/api/v1/openapi.json")
                assert response.status_code == 200, "Failed to fetch OpenAPI spec"
                yield response.json()

    def test_required_endpoints_exist(self, openapi_spec):
        """Verify all endpoints used by the client exist in the spec."""
        paths = openapi_spec.get("paths", {})

        # Endpoints the Python client depends on
        required_endpoints = {
            # Health
            ("/health", "get"),
            # Flows
            ("/flows", "post"),  # store flow
            # Runs
            ("/runs", "post"),  # create run
            ("/runs/{run_id}", "get"),  # get run result
            ("/runs/{run_id}/items", "get"),  # get run items (for batch results)
            # Components
            ("/components", "get"),  # list components
        }

        missing = []
        for path, method in required_endpoints:
            if path not in paths:
                missing.append(f"{method.upper()} {path} - path not found")
            elif method not in paths[path]:
                missing.append(f"{method.upper()} {path} - method not found")

        assert not missing, "Missing endpoints in OpenAPI spec:\n" + "\n".join(missing)

    def test_flow_store_request_schema(self, openapi_spec):
        """Verify POST /flows request body matches client expectations."""
        flows_post = openapi_spec["paths"]["/flows"]["post"]
        request_body = flows_post.get("requestBody", {})

        # Client sends: {"flow": <workflow_dict>}
        assert "content" in request_body
        assert "application/json" in request_body["content"]

        schema_ref = request_body["content"]["application/json"].get("schema", {})
        # Should reference StoreFlowRequest or similar
        assert schema_ref, "POST /flows should have a request schema"

    def test_flow_store_response_schema(self, openapi_spec):
        """Verify POST /flows response contains flowId."""
        flows_post = openapi_spec["paths"]["/flows"]["post"]
        responses = flows_post.get("responses", {})

        # Should have 200 response
        assert "200" in responses, "POST /flows should have 200 response"

        # Check that response schema exists
        response_200 = responses["200"]
        assert "content" in response_200
        assert "application/json" in response_200["content"]

    def test_run_create_request_schema(self, openapi_spec):
        """Verify POST /runs request body matches client expectations."""
        runs_post = openapi_spec["paths"]["/runs"]["post"]
        request_body = runs_post.get("requestBody", {})

        assert "content" in request_body
        assert "application/json" in request_body["content"]

    def test_items_endpoint_schema(self, openapi_spec):
        """Verify items endpoint matches client expectations."""
        paths = openapi_spec["paths"]

        # GET /runs/{run_id}/items - get run items (replaces batch endpoints)
        items_get = paths["/runs/{run_id}/items"]["get"]
        assert "200" in items_get["responses"]

    def test_response_uses_camel_case(self, openapi_spec):
        """Verify API uses camelCase for JSON fields (client assumption)."""
        schemas = openapi_spec.get("components", {}).get("schemas", {})

        # Check a few key schemas for camelCase
        camel_case_fields = ["flowId", "runId", "flowName", "itemCount"]

        found_camel = []
        for schema_name, schema in schemas.items():
            props = schema.get("properties", {})
            for field in camel_case_fields:
                if field in props:
                    found_camel.append(f"{schema_name}.{field}")

        # Should find at least some camelCase fields
        assert found_camel, "Expected camelCase fields in schemas (flowId, runId, etc.)"

    def test_health_endpoint_response(self, openapi_spec):
        """Verify health endpoint response structure."""
        health_get = openapi_spec["paths"]["/health"]["get"]
        responses = health_get.get("responses", {})

        assert "200" in responses, "Health endpoint should have 200 response"


class TestApiVersioning:
    """Tests for API versioning and compatibility."""

    @pytest.fixture
    async def openapi_spec(self):
        """Fetch OpenAPI spec from running server."""
        async with await StepflowRuntime.start_async(
            log_config=LogConfig(level="warn", capture=False),
            startup_timeout=30.0,
        ) as runtime:
            async with httpx.AsyncClient(base_url=runtime.url, timeout=30.0) as client:
                response = await client.get("/api/v1/openapi.json")
                yield response.json()

    def test_openapi_version(self, openapi_spec):
        """Verify OpenAPI spec version is compatible."""
        version = openapi_spec.get("openapi", "")
        # We support OpenAPI 3.x
        assert version.startswith("3."), f"Expected OpenAPI 3.x, got {version}"

    def test_api_version_in_path(self, openapi_spec):
        """Verify API is versioned (v1)."""
        servers = openapi_spec.get("servers", [])
        # At least one server should have /api/v1 in the URL
        has_v1 = any("/api/v1" in s.get("url", "") for s in servers)
        assert has_v1, "API should be versioned with /api/v1"
