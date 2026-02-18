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

"""Pytest fixtures for recovery integration tests."""

from __future__ import annotations

import pytest

from helpers import (
    ORCH1_URL,
    ORCH2_URL,
    clear_tracker,
    compose_down,
    compose_up,
    wait_for_health,
)


@pytest.fixture(scope="function")
def compose_env():
    """Start a fresh Docker Compose environment for each test.

    Tears down volumes for clean state, builds and starts all services,
    and waits for health checks before yielding. Retries once on startup
    failure (transient SQLite migration races between orchestrators).
    """
    compose_down()
    try:
        compose_up()
    except Exception:
        compose_down()
        compose_up()

    wait_for_health(ORCH1_URL, timeout=60)
    wait_for_health(ORCH2_URL, timeout=60)

    clear_tracker()

    yield {
        "orch1_url": ORCH1_URL,
        "orch2_url": ORCH2_URL,
    }

    # Leave environment up on failure for debugging.
    # Next test's setup (compose_down) will clean up.
