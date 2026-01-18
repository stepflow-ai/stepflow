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

"""Stepflow Orchestrator - Launch Stepflow server as a subprocess.

This package provides a convenient way to launch the Stepflow orchestrator
binary as a subprocess with configuration management, health checking,
and graceful shutdown.

Example with default config:
    async with StepflowOrchestrator.start() as client:
        flow_id = client.store_flow(workflow)
        result = client.run(flow_id, input_data)

Example with custom config:
    config = OrchestratorConfig(port=8080, log_level="debug")
    async with StepflowOrchestrator.start(config) as client:
        flow_id = client.store_flow(workflow)
        result = client.run(flow_id, input_data)
"""

# Re-export StepflowClient for convenience
from stepflow_py import StepflowClient

from stepflow_orchestrator.orchestrator import (
    OrchestratorConfig,
    StepflowOrchestrator,
)

__all__ = [
    "StepflowOrchestrator",
    "OrchestratorConfig",
    "StepflowClient",
]
