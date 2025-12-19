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

"""Tests for logging utilities."""

from stepflow_runtime.logging import LogConfig


class TestLogConfig:
    def test_default_config(self):
        config = LogConfig()
        assert config.level == "info"
        assert config.format == "json"
        assert config.capture is True
        assert config.python_logger == "stepflow.server"
        assert config.file_path is None
        assert config.otlp_endpoint is None

    def test_custom_config(self):
        config = LogConfig(
            level="debug",
            format="text",
            capture=False,
            python_logger="myapp.stepflow",
        )
        assert config.level == "debug"
        assert config.format == "text"
        assert config.capture is False
        assert config.python_logger == "myapp.stepflow"

    def test_to_cli_args_stdout(self):
        config = LogConfig(level="debug", format="json")
        args = config.to_cli_args()
        assert "--log-level" in args
        assert "debug" in args
        assert "--log-format" in args
        assert "json" in args
        assert "--log-destination" in args
        assert "stdout" in args

    def test_to_cli_args_with_file(self):
        from pathlib import Path

        config = LogConfig(file_path=Path("/tmp/stepflow.log"))
        args = config.to_cli_args()
        assert "--log-destination" in args
        assert "file" in args
        assert "--log-file" in args
        assert "/tmp/stepflow.log" in args

    def test_to_cli_args_with_otlp(self):
        config = LogConfig(otlp_endpoint="http://localhost:4317")
        args = config.to_cli_args()
        assert "--log-destination" in args
        assert "otlp" in args
        assert "--otlp-endpoint" in args
        assert "http://localhost:4317" in args
