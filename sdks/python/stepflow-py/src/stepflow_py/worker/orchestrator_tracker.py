# Copyright 2025 DataStax Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License. You may obtain a copy of
# the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations under
# the License.

"""Per-task orchestrator URL tracker with automatic discovery.

Each task may belong to a different run on a different orchestrator.
The tracker is created per-task in ``_handle_task`` and shared across
that task's heartbeat loop, completion, and ``GrpcContext``. When any
operation discovers that the orchestrator has moved (via
``GetOrchestratorForRun``), all operations see the updated URL.

All operations for a task run on the same asyncio event loop — no
locking is needed.
"""

from __future__ import annotations

import logging

import grpc
import grpc.aio

from stepflow_py.proto import (
    GetOrchestratorForRunRequest,
)
from stepflow_py.proto.tasks_pb2_grpc import TasksServiceStub

logger = logging.getLogger(__name__)


class OrchestratorTracker:
    """Mutable tracker for the orchestrator URL of a single task's run.

    Created per-task in ``_handle_task``. Shared across that task's
    heartbeat loop, task completion, and ``GrpcContext`` RPCs. Not
    shared across tasks — each task may belong to a different run on
    a different orchestrator.
    """

    def __init__(
        self,
        url: str,
        run_id: str | None,
        root_run_id: str | None,
        tasks_url: str,
    ):
        self._url = url
        self._run_id = run_id
        self._root_run_id = root_run_id
        self._tasks_url = tasks_url

    @property
    def url(self) -> str:
        """Current orchestrator service URL."""
        return self._url

    @property
    def root_run_id(self) -> str | None:
        """Root run ID for ownership validation in OrchestratorService RPCs."""
        return self._root_run_id

    async def discover(self) -> bool:
        """Re-discover the orchestrator URL via GetOrchestratorForRun.

        Returns True if the URL changed, False otherwise.
        Best-effort: if discovery fails, the URL is unchanged.
        """
        lookup_id = self._root_run_id or self._run_id
        if not lookup_id or not self._tasks_url:
            return False
        try:
            channel = grpc.aio.insecure_channel(self._tasks_url)
            try:
                stub = TasksServiceStub(channel)
                response = await stub.GetOrchestratorForRun(
                    GetOrchestratorForRunRequest(run_id=lookup_id)
                )
                new_url = response.orchestrator_service_url
                if new_url and new_url != self._url:
                    logger.info(
                        "Orchestrator for run %s moved: %s -> %s",
                        lookup_id,
                        self._url,
                        new_url,
                    )
                    self._url = new_url
                    return True
            finally:
                await channel.close()
        except Exception:
            logger.debug(
                "GetOrchestratorForRun failed for run %s",
                lookup_id,
                exc_info=True,
            )
        return False
