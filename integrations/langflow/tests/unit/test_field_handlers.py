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

"""Unit tests for field handlers."""

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import Any
from unittest.mock import MagicMock

import pytest

from stepflow_langflow_integration.exceptions import ExecutionError
from stepflow_langflow_integration.executor.base_executor import BaseExecutor
from stepflow_langflow_integration.executor.field_handlers import (
    DataFrameFieldHandler,
    EnvVarFieldHandler,
    FieldHandler,
    StringCoercionFieldHandler,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


class ConcreteTestExecutor(BaseExecutor):
    """Concrete BaseExecutor subclass for testing _apply_field_handlers."""

    async def _instantiate_component(
        self,
        component_info: dict[str, Any],
    ) -> tuple[Any, str]:
        return component_info.get("instance"), component_info.get("name", "test")


# ---------------------------------------------------------------------------
# EnvVarFieldHandler
# ---------------------------------------------------------------------------


class TestEnvVarFieldHandler:
    def test_matches_load_from_db_true(self):
        handler = EnvVarFieldHandler()
        assert handler.matches({"load_from_db": True, "value": "MY_VAR"}) is True

    def test_no_match_without_load_from_db(self):
        handler = EnvVarFieldHandler()
        assert handler.matches({"type": "str", "value": "hello"}) is False

    def test_no_match_load_from_db_false(self):
        handler = EnvVarFieldHandler()
        assert handler.matches({"load_from_db": False}) is False

    @pytest.mark.asyncio
    async def test_resolves_env_var(self, monkeypatch):
        monkeypatch.setenv("OPENAI_API_KEY", "sk-test-123")
        handler = EnvVarFieldHandler()
        fields = {
            "api_key": (
                "",  # empty value
                {"load_from_db": True, "value": "OPENAI_API_KEY"},
            ),
        }
        result = await handler.prepare(fields, None)
        assert result == {"api_key": "sk-test-123"}

    @pytest.mark.asyncio
    async def test_skips_truthy_value(self, monkeypatch):
        monkeypatch.setenv("OPENAI_API_KEY", "env-value")
        handler = EnvVarFieldHandler()
        fields = {
            "api_key": (
                "already-set",
                {"load_from_db": True, "value": "OPENAI_API_KEY"},
            ),
        }
        result = await handler.prepare(fields, None)
        assert result == {}  # no changes

    @pytest.mark.asyncio
    async def test_raises_on_missing_env_var(self, monkeypatch):
        monkeypatch.delenv("MISSING_VAR", raising=False)
        handler = EnvVarFieldHandler()
        fields = {
            "api_key": ("", {"load_from_db": True, "value": "MISSING_VAR"}),
        }
        with pytest.raises(ExecutionError, match="Environment variable.*not set"):
            await handler.prepare(fields, None)

    @pytest.mark.asyncio
    async def test_multiple_env_vars(self, monkeypatch):
        monkeypatch.setenv("KEY1", "val1")
        monkeypatch.setenv("KEY2", "val2")
        handler = EnvVarFieldHandler()
        fields = {
            "k1": ("", {"load_from_db": True, "value": "KEY1"}),
            "k2": ("", {"load_from_db": True, "value": "KEY2"}),
            "k3": ("existing", {"load_from_db": True, "value": "KEY3"}),
        }
        result = await handler.prepare(fields, None)
        assert result == {"k1": "val1", "k2": "val2"}


# ---------------------------------------------------------------------------
# StringCoercionFieldHandler
# ---------------------------------------------------------------------------


class TestStringCoercionFieldHandler:
    def test_matches_str_type(self):
        handler = StringCoercionFieldHandler()
        assert handler.matches({"type": "str"}) is True

    def test_no_match_other_types(self):
        handler = StringCoercionFieldHandler()
        assert handler.matches({"type": "int"}) is False
        assert handler.matches({"type": "file"}) is False
        assert handler.matches({}) is False

    @pytest.mark.asyncio
    async def test_coerces_message_to_text(self):
        handler = StringCoercionFieldHandler()
        msg = MagicMock()
        msg.__class__ = type("Message", (), {})
        msg.text = "hello world"
        fields = {"input_value": (msg, {"type": "str"})}
        result = await handler.prepare(fields, None)
        assert result == {"input_value": "hello world"}

    @pytest.mark.asyncio
    async def test_passes_through_plain_string(self):
        handler = StringCoercionFieldHandler()
        fields = {"input_value": ("already a string", {"type": "str"})}
        result = await handler.prepare(fields, None)
        assert result == {}  # no changes

    @pytest.mark.asyncio
    async def test_passes_through_non_message_objects(self):
        handler = StringCoercionFieldHandler()
        fields = {"input_value": (42, {"type": "str"})}
        result = await handler.prepare(fields, None)
        assert result == {}


# ---------------------------------------------------------------------------
# DataFrameFieldHandler
# ---------------------------------------------------------------------------


class TestDataFrameFieldHandler:
    def test_matches_dataframe_in_input_types(self):
        handler = DataFrameFieldHandler()
        assert handler.matches({"input_types": ["DataFrame", "Data"]}) is True

    def test_no_match_without_dataframe(self):
        handler = DataFrameFieldHandler()
        assert handler.matches({"input_types": ["Message"]}) is False
        assert handler.matches({}) is False

    @pytest.mark.asyncio
    async def test_converts_data_list(self):
        handler = DataFrameFieldHandler()
        data_list = [
            {"text": "row1", "value": 1},
            {"text": "row2", "value": 2},
        ]
        fields = {
            "data_input": (
                data_list,
                {"input_types": ["DataFrame"]},
            ),
        }
        result = await handler.prepare(fields, None)
        assert "data_input" in result
        assert result["data_input"].__class__.__name__ == "DataFrame"

    @pytest.mark.asyncio
    async def test_skips_empty_list(self):
        handler = DataFrameFieldHandler()
        fields = {"data_input": ([], {"input_types": ["DataFrame"]})}
        result = await handler.prepare(fields, None)
        assert result == {}

    @pytest.mark.asyncio
    async def test_skips_non_list(self):
        handler = DataFrameFieldHandler()
        fields = {"data_input": ("not a list", {"input_types": ["DataFrame"]})}
        result = await handler.prepare(fields, None)
        assert result == {}

    @pytest.mark.asyncio
    async def test_skips_non_data_list(self):
        handler = DataFrameFieldHandler()
        fields = {
            "data_input": (
                [1, 2, 3],  # not Data-like
                {"input_types": ["DataFrame"]},
            ),
        }
        result = await handler.prepare(fields, None)
        assert result == {}

    @pytest.mark.asyncio
    async def test_skips_all_none_list(self):
        handler = DataFrameFieldHandler()
        fields = {
            "data_input": (
                [None, None],  # no non-null items
                {"input_types": ["DataFrame"]},
            ),
        }
        result = await handler.prepare(fields, None)
        assert result == {}


# ---------------------------------------------------------------------------
# _apply_field_handlers
# ---------------------------------------------------------------------------


class TestApplyFieldHandlers:
    @pytest.fixture
    def executor(self):
        return ConcreteTestExecutor()

    @pytest.mark.asyncio
    async def test_runs_handlers_in_order(self, executor):
        """Handlers should be applied sequentially, each seeing previous results."""
        call_order: list[str] = []

        class HandlerA(FieldHandler):
            def matches(self, template_field):
                return template_field.get("handle_a", False)

            async def prepare(self, fields, context):
                call_order.append("A")
                return {k: v + "_A" for k, (v, _) in fields.items()}

        class HandlerB(FieldHandler):
            def matches(self, template_field):
                return template_field.get("handle_b", False)

            async def prepare(self, fields, context):
                call_order.append("B")
                return {k: v + "_B" for k, (v, _) in fields.items()}

        parameters = {"x": "val"}
        template = {"x": {"handle_a": True, "handle_b": True}}
        handlers = [HandlerA(), HandlerB()]
        result = await executor._apply_field_handlers(parameters, template, handlers)

        assert call_order == ["A", "B"]
        assert result["x"] == "val_A_B"

    @pytest.mark.asyncio
    async def test_skips_handlers_with_no_matches(self, executor):
        """activate() should not be called when no fields match."""
        activated = []

        class TrackingHandler(FieldHandler):
            def matches(self, template_field):
                return False  # never matches

            @asynccontextmanager
            async def activate(self) -> AsyncIterator[Any]:
                activated.append(True)
                yield None

            async def prepare(self, fields, context):
                return {}

        parameters = {"x": "val"}
        template = {"x": {"type": "str"}}
        result = await executor._apply_field_handlers(
            parameters, template, [TrackingHandler()]
        )

        assert activated == []  # activate was never called
        assert result == {"x": "val"}

    @pytest.mark.asyncio
    async def test_passes_context_from_activate(self, executor):
        """Context yielded by activate() should be passed to prepare()."""
        received_contexts: list[Any] = []

        class ContextHandler(FieldHandler):
            def matches(self, template_field):
                return True

            @asynccontextmanager
            async def activate(self) -> AsyncIterator[str]:
                yield "my_context"

            async def prepare(self, fields, context):
                received_contexts.append(context)
                return {}

        parameters = {"x": "val"}
        template = {"x": {"type": "str"}}
        await executor._apply_field_handlers(
            parameters, template, [ContextHandler()]
        )

        assert received_contexts == ["my_context"]

    @pytest.mark.asyncio
    async def test_non_dict_template_fields_skipped(self, executor):
        """Fields with non-dict template entries should not be passed to handlers."""

        class AlwaysMatchHandler(FieldHandler):
            def matches(self, template_field):
                return True

            async def prepare(self, fields, context):
                return {k: "changed" for k in fields}

        parameters = {"x": "val", "y": "val2"}
        template = {"x": {"type": "str"}, "y": "direct_value"}
        result = await executor._apply_field_handlers(
            parameters, template, [AlwaysMatchHandler()]
        )

        assert result["x"] == "changed"
        assert result["y"] == "val2"  # not passed to handler

    @pytest.mark.asyncio
    async def test_empty_handlers_is_noop(self, executor):
        parameters = {"x": "val"}
        template = {"x": {"type": "str"}}
        result = await executor._apply_field_handlers(parameters, template, [])
        assert result == {"x": "val"}

    @pytest.mark.asyncio
    async def test_handler_updates_are_merged(self, executor):
        """Only keys returned by prepare() should be updated."""

        class SelectiveHandler(FieldHandler):
            def matches(self, template_field):
                return True

            async def prepare(self, fields, context):
                # Only update "a", leave "b" alone
                return {"a": "updated"}

        parameters = {"a": "orig_a", "b": "orig_b"}
        template = {"a": {"type": "str"}, "b": {"type": "str"}}
        result = await executor._apply_field_handlers(
            parameters, template, [SelectiveHandler()]
        )

        assert result["a"] == "updated"
        assert result["b"] == "orig_b"
