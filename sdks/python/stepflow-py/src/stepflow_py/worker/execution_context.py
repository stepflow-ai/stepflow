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

"""Execution context for the current component invocation.

Provides a single ``ContextVar`` holding an immutable
:class:`ExecutionContext` snapshot.  The framework sets this before each
component call and resets it afterward so that any code—components,
libraries, the blob store—can read the current run / step metadata
without threading a context parameter.

Usage::

    from stepflow_py.worker import execution_context

    ctx = execution_context.get()
    print(ctx.run_id, ctx.step_id)
"""

from __future__ import annotations

from contextvars import ContextVar, Token
from dataclasses import dataclass


@dataclass(frozen=True)
class ExecutionContext:
    """Immutable snapshot of the current execution context."""

    run_id: str | None = None
    step_id: str | None = None
    flow_id: str | None = None
    attempt: int = 1


_DEFAULT = ExecutionContext()

_current: ContextVar[ExecutionContext | None] = ContextVar(
    "stepflow_execution_context", default=None
)


def get() -> ExecutionContext:
    """Return the current execution context."""
    return _current.get() or _DEFAULT


def set_context(
    *,
    run_id: str | None = None,
    step_id: str | None = None,
    flow_id: str | None = None,
    attempt: int = 1,
) -> Token[ExecutionContext | None]:
    """Set the execution context for the current async scope.

    Returns a token that can be passed to :func:`reset_context` to restore
    the previous context.
    """
    ctx = ExecutionContext(
        run_id=run_id, step_id=step_id, flow_id=flow_id, attempt=attempt
    )
    return _current.set(ctx)


def reset_context(token: Token[ExecutionContext | None]) -> None:
    """Restore the previous execution context."""
    _current.reset(token)
