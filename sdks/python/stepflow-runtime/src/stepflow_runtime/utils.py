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

"""Pure Python utilities for the Stepflow runtime."""

import platform
import socket
from pathlib import Path


def get_binary_name() -> str:
    """Get the stepflow-worker binary name for the current platform."""
    if platform.system() == "Windows":
        return "stepflow-worker.exe"
    return "stepflow-worker"


def get_binary_path() -> Path:
    """Get the path to the bundled stepflow-worker binary.

    Returns:
        Path to the stepflow-worker binary

    Raises:
        FileNotFoundError: If the binary is not found
    """
    # Try importlib.resources first (Python 3.9+)
    try:
        from importlib.resources import files

        package_dir = files("stepflow_runtime")
        binary_path = Path(str(package_dir / "bin" / get_binary_name()))

        if binary_path.exists():
            return binary_path
    except Exception:
        pass

    # Fallback: use __file__ to find the binary
    package_dir = Path(__file__).parent
    binary_path = package_dir / "bin" / get_binary_name()

    if binary_path.exists():
        return binary_path

    raise FileNotFoundError(
        f"stepflow-worker binary not found at {binary_path}. "
        "This may indicate a packaging issue. "
        "Please ensure you installed stepflow-runtime from an official wheel."
    )


def find_free_port() -> int:
    """Find an available TCP port.

    Returns:
        An available port number
    """
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        s.listen(1)
        return s.getsockname()[1]


def is_port_in_use(port: int, host: str = "127.0.0.1") -> bool:
    """Check if a port is in use.

    Args:
        port: Port number to check
        host: Host address (default: 127.0.0.1)

    Returns:
        True if the port is in use, False otherwise
    """
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        try:
            s.bind((host, port))
            return False
        except OSError:
            return True
