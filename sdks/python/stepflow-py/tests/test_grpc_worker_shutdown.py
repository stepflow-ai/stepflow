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

"""Tests for gRPC worker graceful shutdown and in-flight task tracking."""

from __future__ import annotations

import asyncio

import pytest

from stepflow_py.worker.grpc_worker import _InFlightTasks

# --- _InFlightTasks registry tests ---


@pytest.mark.asyncio
async def test_register_and_unregister():
    """Tasks can be registered and unregistered."""
    registry = _InFlightTasks()

    # Use a mock TaskAssignment (just needs to exist)
    assignment = object()
    task = asyncio.create_task(asyncio.sleep(100))

    await registry.register("task-1", assignment, task)
    snap = await registry.snapshot()
    assert len(snap) == 1
    assert snap[0][0] == "task-1"

    await registry.unregister("task-1")
    snap = await registry.snapshot()
    assert len(snap) == 0

    task.cancel()
    try:
        await task
    except asyncio.CancelledError:
        pass


@pytest.mark.asyncio
async def test_unregister_nonexistent_is_noop():
    """Unregistering a nonexistent task does not raise."""
    registry = _InFlightTasks()
    await registry.unregister("nonexistent")  # should not raise


@pytest.mark.asyncio
async def test_snapshot_returns_copy():
    """Snapshot returns current state without modifying the registry."""
    registry = _InFlightTasks()

    tasks = []
    for i in range(3):
        assignment = object()
        task = asyncio.create_task(asyncio.sleep(100))
        tasks.append(task)
        await registry.register(f"task-{i}", assignment, task)

    snap = await registry.snapshot()
    assert len(snap) == 3

    # Unregister one — snapshot already taken should be unaffected
    await registry.unregister("task-0")
    assert len(snap) == 3  # original snapshot unchanged

    snap2 = await registry.snapshot()
    assert len(snap2) == 2

    for t in tasks:
        t.cancel()
        try:
            await t
        except asyncio.CancelledError:
            pass


@pytest.mark.asyncio
async def test_register_replaces_existing():
    """Re-registering the same task_id replaces the previous entry."""
    registry = _InFlightTasks()

    assignment1 = object()
    task1 = asyncio.create_task(asyncio.sleep(100))
    await registry.register("task-1", assignment1, task1)

    assignment2 = object()
    task2 = asyncio.create_task(asyncio.sleep(100))
    await registry.register("task-1", assignment2, task2)

    snap = await registry.snapshot()
    assert len(snap) == 1
    assert snap[0][1] is assignment2  # replaced with second entry

    for t in [task1, task2]:
        t.cancel()
        try:
            await t
        except asyncio.CancelledError:
            pass
