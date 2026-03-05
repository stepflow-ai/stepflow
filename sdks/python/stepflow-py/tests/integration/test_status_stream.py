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

"""Integration tests for SSE status event streaming.

Tests cover:
- Basic streaming of execution events
- Resume from a sequence number (reconnection)
- Event type filtering
- Recovery: stream events that span before and after an orchestrator restart

These tests require STEPFLOW_DEV_BINARY to be set.
"""

from __future__ import annotations

import os
import tempfile
from pathlib import Path

import pytest
import pytest_asyncio

from stepflow_py import StatusEvent, StepflowClient
from stepflow_py.config import (
    BuiltinPluginConfig,
    RouteRule,
    SqliteStoreConfig,
    StepflowConfig,
)


def _skip_if_no_binary():
    if not os.environ.get("STEPFLOW_DEV_BINARY"):
        pytest.skip("STEPFLOW_DEV_BINARY not set")


def _builtin_config(storage_config=None):
    """Create a minimal config with only builtin components."""
    return StepflowConfig(
        plugins={"builtin": BuiltinPluginConfig()},
        routes={"/builtin/{*component}": [RouteRule(plugin="builtin")]},
        storageConfig=storage_config,
    )


# A simple workflow that produces step events.
SIMPLE_WORKFLOW = {
    "name": "sse_test",
    "steps": [
        {
            "id": "create_msg",
            "component": "/builtin/create_messages",
            "input": {"user_prompt": "hello"},
        },
    ],
    "output": {"$step": "create_msg"},
}


# =============================================================================
# Basic SSE streaming tests (in-memory storage, module-scoped orchestrator)
# =============================================================================


@pytest_asyncio.fixture(scope="module")
async def client():
    _skip_if_no_binary()
    config = _builtin_config()
    async with StepflowClient.local(config, startup_timeout=60.0) as c:
        yield c


@pytest_asyncio.fixture(scope="module")
async def completed_run(client):
    """Submit and complete a run, returning (flow_id, run_id)."""
    store_resp = await client.store_flow(SIMPLE_WORKFLOW)
    run_resp = await client.run(store_resp.flow_id, {"x": 1}, timeout=60.0)
    return store_resp.flow_id, run_resp.run_id


@pytest.mark.asyncio
async def test_stream_completed_run(client, completed_run):
    """Stream events for a completed run — should replay and close."""
    _, run_id = completed_run
    events: list[StatusEvent] = []
    async for ev in client.status_events(run_id):
        events.append(ev)

    event_types = [e.event for e in events]
    assert "run_created" in event_types
    assert "run_completed" in event_types

    # All events should have sequence numbers and timestamps
    for ev in events:
        assert ev.id is not None, f"Event {ev.event} missing id"
        assert ev.sequence_number is not None, (
            f"Event {ev.event} missing sequenceNumber"
        )
        assert ev.timestamp is not None, f"Event {ev.event} missing timestamp"

    # Sequence numbers should be non-decreasing
    seqs = [ev.sequence_number for ev in events]
    for i in range(1, len(seqs)):
        assert seqs[i] >= seqs[i - 1], f"Sequence not monotonic: {seqs}"


@pytest.mark.asyncio
async def test_stream_resume_with_since(client, completed_run):
    """Resume stream from a midpoint — should skip earlier events."""
    _, run_id = completed_run

    # Get all events first
    all_events: list[StatusEvent] = []
    async for ev in client.status_events(run_id):
        all_events.append(ev)
    assert len(all_events) >= 3, f"Expected >= 3 events, got {len(all_events)}"

    # Resume from after the second event
    resume_from = all_events[1].sequence_number + 1
    resumed: list[StatusEvent] = []
    async for ev in client.status_events(run_id, since=resume_from):
        resumed.append(ev)

    assert len(resumed) < len(all_events), (
        f"Resumed should have fewer events: {len(resumed)} vs {len(all_events)}"
    )
    # All resumed events should have seq >= resume_from
    for ev in resumed:
        assert ev.sequence_number >= resume_from
    # Should still end with run_completed
    assert resumed[-1].event == "run_completed"


@pytest.mark.asyncio
async def test_stream_event_type_filter(client, completed_run):
    """Filter stream to specific event types."""
    _, run_id = completed_run
    events: list[StatusEvent] = []
    async for ev in client.status_events(run_id, event_types=["run_completed"]):
        events.append(ev)

    assert len(events) >= 1
    for ev in events:
        assert ev.event == "run_completed"


@pytest.mark.asyncio
async def test_stream_include_results(client, completed_run):
    """Include results in step_completed events."""
    _, run_id = completed_run

    # With results
    with_results: list[StatusEvent] = []
    async for ev in client.status_events(run_id, include_results=True):
        with_results.append(ev)

    step_completed = [e for e in with_results if e.event == "step_completed"]
    assert len(step_completed) > 0, "Should have step_completed events"
    for ev in step_completed:
        assert "result" in ev.data, "step_completed should include result"

    # Without results (default)
    without_results: list[StatusEvent] = []
    async for ev in client.status_events(run_id):
        without_results.append(ev)

    step_completed = [e for e in without_results if e.event == "step_completed"]
    for ev in step_completed:
        assert ev.data.get("result") is None, (
            "step_completed should not include result by default"
        )


# =============================================================================
# Recovery streaming tests (SQLite storage, per-test orchestrator lifecycle)
# =============================================================================


@pytest.mark.asyncio
async def test_stream_after_restart():
    """Stream events for a run that completed before orchestrator restart.

    Verifies that the journal is durable and the SSE stream can replay
    events from a previous orchestrator lifetime.
    """
    _skip_if_no_binary()

    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = Path(tmpdir) / "state.db"
        config = _builtin_config(
            storage_config=SqliteStoreConfig(
                databaseUrl=f"sqlite:{db_path}?mode=rwc",
                autoMigrate=True,
            ),
        )

        # Phase 1: run to completion, collect events
        async with StepflowClient.local(config, startup_timeout=60.0) as client:
            store_resp = await client.store_flow(SIMPLE_WORKFLOW)
            run_resp = await client.run(store_resp.flow_id, {"x": 1}, timeout=60.0)
            run_id = run_resp.run_id

            original_events: list[StatusEvent] = []
            async for ev in client.status_events(run_id):
                original_events.append(ev)

        # Orchestrator is now shut down. Database persists.
        assert db_path.exists(), "SQLite database should persist"

        # Phase 2: restart, stream the same run
        async with StepflowClient.local(config, startup_timeout=60.0) as client:
            replayed_events: list[StatusEvent] = []
            async for ev in client.status_events(run_id):
                replayed_events.append(ev)

        # Should get the same events (journal replay)
        assert len(replayed_events) == len(original_events), (
            f"Replayed {len(replayed_events)} events, expected {len(original_events)}"
        )
        # Event types should match
        assert [e.event for e in replayed_events] == [e.event for e in original_events]
        # Sequence numbers should match
        assert [e.id for e in replayed_events] == [e.id for e in original_events]


@pytest.mark.asyncio
async def test_stream_resume_after_restart():
    """Resume a stream from a midpoint after orchestrator restart.

    Simulates a client that was receiving events, lost connection when
    the orchestrator went down, and reconnects with ?since=<last_id>.
    """
    _skip_if_no_binary()

    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = Path(tmpdir) / "state.db"
        config = _builtin_config(
            storage_config=SqliteStoreConfig(
                databaseUrl=f"sqlite:{db_path}?mode=rwc",
                autoMigrate=True,
            ),
        )

        # Phase 1: run to completion, get all events
        async with StepflowClient.local(config, startup_timeout=60.0) as client:
            store_resp = await client.store_flow(SIMPLE_WORKFLOW)
            run_resp = await client.run(store_resp.flow_id, {"x": 1}, timeout=60.0)
            run_id = run_resp.run_id

            all_events: list[StatusEvent] = []
            async for ev in client.status_events(run_id):
                all_events.append(ev)

        assert len(all_events) >= 3

        # Phase 2: restart and resume from midpoint
        midpoint_seq = all_events[1].sequence_number + 1

        async with StepflowClient.local(config, startup_timeout=60.0) as client:
            resumed: list[StatusEvent] = []
            async for ev in client.status_events(run_id, since=midpoint_seq):
                resumed.append(ev)

        # Should have fewer events (skipped early ones)
        assert len(resumed) < len(all_events)
        # All resumed events should have seq >= midpoint
        for ev in resumed:
            assert ev.sequence_number >= midpoint_seq
        # Should end with run_completed
        assert resumed[-1].event == "run_completed"
