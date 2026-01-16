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

"""Log capture and forwarding for the Stepflow runtime."""

import json
import logging
import threading
from collections import deque
from dataclasses import dataclass, field
from datetime import datetime
from io import TextIOWrapper
from pathlib import Path
from typing import IO

from stepflow_core import LogEntry

# Map log level strings to Python logging levels
LOG_LEVEL_MAP = {
    "trace": logging.DEBUG,
    "debug": logging.DEBUG,
    "info": logging.INFO,
    "warn": logging.WARNING,
    "warning": logging.WARNING,
    "error": logging.ERROR,
    "critical": logging.CRITICAL,
    "off": logging.NOTSET,
}


@dataclass
class LogConfig:
    """Configuration for stepflow-worker logging.

    Attributes:
        level: Log level for the server (trace, debug, info, warn, error)
        format: Log format (json or text)
        capture: Whether to capture logs to Python
        python_logger: Name of the Python logger to forward logs to
        file_path: Optional path to write logs to a file
        otlp_endpoint: Optional OTLP endpoint for log export
    """

    level: str = "info"
    format: str = "json"
    capture: bool = True
    python_logger: str = "stepflow.server"
    file_path: Path | None = None
    otlp_endpoint: str | None = None

    def to_cli_args(self) -> list[str]:
        """Convert config to CLI arguments for stepflow-worker."""
        args = [
            "--log-level",
            self.level,
            "--log-format",
            self.format,
        ]

        if self.file_path:
            args.extend(
                ["--log-destination", "file", "--log-file", str(self.file_path)]
            )
        elif self.otlp_endpoint:
            args.extend(
                ["--log-destination", "otlp", "--otlp-endpoint", self.otlp_endpoint]
            )
        else:
            args.extend(["--log-destination", "stdout"])

        return args


@dataclass
class LogForwarder:
    """Reads logs from subprocess and forwards to Python logging.

    This class runs a background thread that reads JSON log lines from
    the subprocess stdout and forwards them to Python's logging system.
    It also maintains a buffer of recent log entries for programmatic access.
    """

    pipe: IO[str] | TextIOWrapper
    logger_name: str = "stepflow.server"
    max_entries: int = 1000

    _logger: logging.Logger = field(init=False)
    _thread: threading.Thread = field(init=False)
    _entries: deque[LogEntry] = field(init=False)
    _running: bool = field(init=False, default=False)
    _lock: threading.Lock = field(init=False, default_factory=threading.Lock)

    def __post_init__(self) -> None:
        self._logger = logging.getLogger(self.logger_name)
        self._entries = deque(maxlen=self.max_entries)
        self._thread = threading.Thread(
            target=self._run, daemon=True, name="stepflow-log-forwarder"
        )

    def start(self) -> None:
        """Start the log forwarding thread."""
        if self._running:
            return
        self._running = True
        self._thread.start()

    def stop(self) -> None:
        """Stop the log forwarding thread."""
        self._running = False
        # Thread will exit when pipe is closed or on next read timeout

    def get_recent(self, limit: int = 100) -> list[LogEntry]:
        """Get recent log entries.

        Args:
            limit: Maximum number of entries to return

        Returns:
            List of LogEntry objects, most recent last
        """
        with self._lock:
            entries = list(self._entries)
        return entries[-limit:] if limit < len(entries) else entries

    def _run(self) -> None:
        """Main loop for reading and forwarding logs."""
        try:
            for line in self.pipe:
                if not self._running:
                    break

                line = line.strip()
                if not line:
                    continue

                entry = self._parse_log_line(line)
                if entry:
                    self._forward_entry(entry)
        except Exception:
            # Pipe closed or error reading - this is expected on shutdown
            pass

    def _parse_log_line(self, line: str) -> LogEntry | None:
        """Parse a JSON log line into a LogEntry."""
        try:
            data = json.loads(line)

            # Parse timestamp
            timestamp_str = data.get("timestamp", "")
            try:
                timestamp = datetime.fromisoformat(timestamp_str.replace("Z", "+00:00"))
            except Exception:
                timestamp = datetime.now()

            return LogEntry(
                timestamp=timestamp,
                level=data.get("level", "info").lower(),
                message=data.get("message", ""),
                target=data.get("target"),
                trace_id=data.get("trace_id"),
                span_id=data.get("span_id"),
                run_id=data.get("run_id"),
                step_id=data.get("step_id"),
                extra={
                    k: v
                    for k, v in data.items()
                    if k
                    not in {
                        "timestamp",
                        "level",
                        "message",
                        "target",
                        "trace_id",
                        "span_id",
                        "run_id",
                        "step_id",
                    }
                },
            )
        except json.JSONDecodeError:
            # Not JSON - treat as plain text
            return LogEntry(
                timestamp=datetime.now(),
                level="info",
                message=line,
            )
        except Exception:
            return None

    def _forward_entry(self, entry: LogEntry) -> None:
        """Forward a log entry to Python logging and buffer."""
        # Add to buffer
        with self._lock:
            self._entries.append(entry)

        # Forward to Python logger
        level = LOG_LEVEL_MAP.get(entry.level, logging.INFO)
        extra = {
            "target": entry.target,
            "trace_id": entry.trace_id,
            "span_id": entry.span_id,
            "run_id": entry.run_id,
            "step_id": entry.step_id,
            **entry.extra,
        }
        self._logger.log(level, entry.message, extra=extra)
