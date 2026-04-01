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

"""Vsock/Unix socket transport for Stepflow workers.

Receives tasks as length-delimited protobuf VsockTaskEnvelope messages over
vsock or Unix socket connections. Each connection carries one task. The worker
accepts connections in a loop, allowing VM reuse (e.g., after snapshot/restore).

Supports two modes:
- Multi-task (default): Accepts connections in a loop until the process exits.
- One-shot (--oneshot): Exits after completing the first task.

Usage:
    # In a Firecracker VM (vsock):
    python -m stepflow_py.worker.vsock_worker --vsock-port 5000

    # One-shot mode:
    python -m stepflow_py.worker.vsock_worker --vsock-port 5000 --oneshot

    # In dev mode (Unix socket):
    python -m stepflow_py.worker.vsock_worker --socket /tmp/stepflow.sock
"""

from __future__ import annotations

import argparse
import asyncio
import logging
import os
import socket
import struct
from typing import TYPE_CHECKING

from stepflow_py.proto import VsockTaskEnvelope

if TYPE_CHECKING:
    from stepflow_py.worker.server import StepflowServer

logger = logging.getLogger(__name__)

# Ready signal printed to stdout when the worker is listening and ready
# for tasks. Used by the Firecracker proxy to know when to snapshot the VM.
READY_SIGNAL = "STEPFLOW_VSOCK_READY"

# Length prefix: 4-byte big-endian unsigned int
_LENGTH_FMT = ">I"
_LENGTH_SIZE = struct.calcsize(_LENGTH_FMT)
_MAX_MESSAGE_SIZE = 16 * 1024 * 1024  # 16 MiB


async def _read_exact(reader: asyncio.StreamReader, n: int) -> bytes | None:
    """Read exactly n bytes, returning None on EOF."""
    data = b""
    while len(data) < n:
        chunk = await reader.read(n - len(data))
        if not chunk:
            if not data:
                return None  # Clean EOF
            raise ConnectionError(f"Unexpected EOF: got {len(data)}/{n} bytes")
        data += chunk
    return data


async def _read_envelope(reader: asyncio.StreamReader) -> VsockTaskEnvelope | None:
    """Read a length-delimited protobuf VsockTaskEnvelope.

    Returns None on end-of-stream (connection closed).
    """
    len_bytes = await _read_exact(reader, _LENGTH_SIZE)
    if len_bytes is None:
        return None

    (msg_len,) = struct.unpack(_LENGTH_FMT, len_bytes)
    if msg_len > _MAX_MESSAGE_SIZE:
        raise ValueError(
            f"Message too large: {msg_len} bytes (max {_MAX_MESSAGE_SIZE})"
        )

    msg_bytes = await _read_exact(reader, msg_len)
    if msg_bytes is None:
        raise ConnectionError("Unexpected EOF reading message body")

    envelope = VsockTaskEnvelope()
    envelope.ParseFromString(msg_bytes)
    return envelope


async def run_vsock_worker(
    server: StepflowServer,
    *,
    vsock_port: int | None = None,
    socket_path: str | None = None,
    oneshot: bool = False,
    max_concurrent: int = 1,
) -> None:
    """Run the vsock/socket worker.

    Accepts connections in a loop. Each connection carries one or more tasks
    as sequential length-delimited protobuf messages. After EOF on a connection,
    the worker accepts the next connection (unless --oneshot).

    This loop structure is critical for snapshot/restore: the snapshot captures
    the worker blocked in accept(), and the restored VM immediately accepts
    the next connection.
    """
    if vsock_port is not None:
        listener = await _create_vsock_listener(vsock_port)
    elif socket_path is not None:
        listener = await _create_unix_listener(socket_path)
    else:
        raise ValueError("Either --vsock-port or --socket must be specified")

    worker_id = f"vsock-worker-{os.getpid()}"
    semaphore = asyncio.Semaphore(max_concurrent)
    tasks_processed = 0

    try:
        while True:
            # Flush ARP cache before accepting — critical for snapshot/restore.
            # The snapshot captures the worker blocked in accept() with a clean
            # ARP cache. After restore into a new namespace, the first packet
            # triggers a fresh ARP lookup for the new gateway.
            _flush_arp_cache()

            # Accept a connection (blocks until a client connects)
            import time as _time

            t_accept = _time.monotonic()
            reader, writer = await _accept_connection(listener)
            t_connected = _time.monotonic()
            logger.info("TIMING: accept_wait=%.1fms", (t_connected - t_accept) * 1000)

            try:
                # Process all tasks on this connection
                while True:
                    envelope = await _read_envelope(reader)
                    t_envelope = _time.monotonic()
                    if envelope is None:
                        logger.info("End of stream on connection")
                        break

                    if not envelope.HasField("assignment"):
                        logger.warning("Received envelope without assignment, skipping")
                        continue

                    task = envelope.assignment
                    logger.info(
                        "TIMING: envelope_read=%.1fms task=%s component=%s",
                        (t_envelope - t_connected) * 1000,
                        task.task_id,
                        task.execute.component_id if task.HasField("execute") else "?",
                    )

                    # Override blob URL from envelope if provided
                    if envelope.blob_url:
                        import stepflow_py.worker.task_handler as th

                        th._BLOB_URL = envelope.blob_url

                    # handle_task releases the semaphore in its finally block
                    await semaphore.acquire()

                    from stepflow_py.worker.task_handler import handle_task

                    t_before_handle = _time.monotonic()
                    await handle_task(
                        server=server,
                        task=task,
                        semaphore=semaphore,
                        worker_id=worker_id,
                        queue_name="vsock",
                        tracer_name="stepflow-vsock-worker",
                        tasks_url=envelope.tasks_url,
                    )
                    t_after_handle = _time.monotonic()

                    tasks_processed += 1
                    logger.info(
                        "TIMING: handle_task=%.1fms (claim+execute+complete)",
                        (t_after_handle - t_before_handle) * 1000,
                    )
                    logger.info(
                        "Task %s completed (%d total)",
                        task.task_id,
                        tasks_processed,
                    )

                    if oneshot:
                        logger.info("One-shot mode — exiting after first task")
                        return
            finally:
                writer.close()
                try:
                    await writer.wait_closed()
                except Exception:
                    pass

    finally:
        # Clean up listener
        listener.close()
        # Clean up socket file if we created it
        if socket_path and os.path.exists(socket_path):
            try:
                os.unlink(socket_path)
            except OSError:
                pass


def _flush_arp_cache() -> None:
    """Flush the kernel ARP cache. Called before accept() so that snapshots
    capture a clean state. After restore, the first outbound packet
    triggers a fresh ARP for the new gateway."""
    import subprocess

    try:
        subprocess.run(
            ["ip", "neigh", "flush", "all"],
            capture_output=True,
            timeout=5,
        )
    except Exception:
        pass  # Best-effort; may fail on non-Linux or without permissions


async def _create_vsock_listener(port: int) -> socket.socket:
    """Create and bind a vsock listener socket."""
    VMADDR_CID_ANY = 0xFFFFFFFF
    af_vsock = getattr(socket, "AF_VSOCK", None)
    if af_vsock is None:
        raise RuntimeError("AF_VSOCK not available on this platform")
    sock = socket.socket(af_vsock, socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind((VMADDR_CID_ANY, port))
    sock.listen(5)
    sock.setblocking(False)

    logger.info("Listening on vsock port %d", port)
    print(READY_SIGNAL, flush=True)
    return sock


async def _create_unix_listener(path: str) -> socket.socket:
    """Create a Unix domain socket listener."""
    if os.path.exists(path):
        os.unlink(path)

    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind(path)
    sock.listen(5)
    sock.setblocking(False)

    logger.info("Listening on Unix socket %s", path)
    print(READY_SIGNAL, flush=True)
    return sock


async def _accept_connection(
    listener: socket.socket,
) -> tuple[asyncio.StreamReader, asyncio.StreamWriter]:
    """Accept a single connection from the listener socket."""
    loop = asyncio.get_running_loop()
    conn, addr = await loop.sock_accept(listener)
    logger.info("Accepted connection from %s", addr)
    reader, writer = await asyncio.open_connection(sock=conn)
    return reader, writer


def main() -> None:
    """CLI entry point for the vsock worker."""
    from stepflow_py.worker.observability import setup_observability

    setup_observability()

    parser = argparse.ArgumentParser(
        description="Stepflow vsock/socket worker",
    )
    transport_group = parser.add_mutually_exclusive_group(required=True)
    transport_group.add_argument(
        "--vsock-port",
        type=int,
        help="AF_VSOCK port to listen on (for Firecracker VMs)",
    )
    transport_group.add_argument(
        "--socket",
        type=str,
        help="Unix socket path to listen on (for dev mode)",
    )
    parser.add_argument(
        "--oneshot",
        action="store_true",
        help="Exit after completing the first task",
    )
    parser.add_argument(
        "--max-concurrent",
        type=int,
        default=1,
        help="Max concurrent tasks (default: 1)",
    )

    args = parser.parse_args()

    # Import the server with registered components
    from stepflow_py.worker.main import server

    asyncio.run(
        run_vsock_worker(
            server=server,
            vsock_port=args.vsock_port,
            socket_path=args.socket,
            oneshot=args.oneshot,
            max_concurrent=args.max_concurrent,
        )
    )


if __name__ == "__main__":
    main()
