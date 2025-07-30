# Licensed to the Apache Software Foundation (ASF) under one or more contributor
# license agreements.  See the NOTICE file distributed with this work for
# additional information regarding copyright ownership.  The ASF licenses this
# file to you under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License.  You may obtain a copy of
# the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
# License for the specific language governing permissions and limitations under
# the License.


import msgspec
import pytest
from pytest_mock import MockerFixture

from stepflow_py.context import StepflowContext
from stepflow_py.exceptions import StepflowValueError
from stepflow_py.udf import _compile_function, _InputWrapper


class DummyContext:
    async def get_blob(self, blob_id):
        return self.blobs[blob_id]

    def __init__(self, blobs):
        self.blobs = blobs


def make_schema():
    return {
        "type": "object",
        "properties": {"a": {"type": "integer"}, "b": {"type": "integer"}},
        "required": ["a", "b"],
    }


@pytest.fixture
def mock_context(mocker: MockerFixture) -> StepflowContext:
    """
    Mocks my_module.MyClass with autospec=True.
    """
    mock_context_class = mocker.patch(
        "stepflow_py.context.StepflowContext", autospec=True
    )
    mock_instance = mock_context_class.return_value
    mock_instance.session_id = "test_session"
    return mock_instance  # type: ignore


def test_input_wrapper_access():
    data = {"a": 1, "b": {"c": 2}}
    wrapper = _InputWrapper(data)
    assert wrapper.a == 1
    assert wrapper["a"] == 1
    assert wrapper.b.c == 2
    assert wrapper["b"].c == 2
    with pytest.raises(StepflowValueError):
        _ = wrapper.x


@pytest.mark.asyncio
async def test_compile_lambda_body(mock_context):
    code = "input.a + input.b"

    func = _compile_function(code, None, make_schema())
    result = await func({"a": 2, "b": 3}, context=mock_context)
    assert result == 5


@pytest.mark.asyncio
async def test_compile_function_body(mock_context):
    code = """
res = input.a * input.b
return res
"""
    func = _compile_function(code, None, make_schema())
    result = await func({"a": 2, "b": 4}, context=mock_context)
    assert result == 8


@pytest.mark.asyncio
async def test_compile_function_def_sync(mock_context):
    code = """
def myfunc(input):
    return input.a - input.b
"""
    func = _compile_function(code, "myfunc", make_schema())
    result = await func({"a": 5, "b": 3}, context=mock_context)
    assert result == 2


@pytest.mark.asyncio
async def test_compile_function_def_sync_context(mock_context):
    code = """
def myfunc(input, context):
    return f"{input.a} + {input.b}: {context.session_id}"
"""
    func = _compile_function(code, "myfunc", make_schema())
    result = await func({"a": 1, "b": 2}, context=mock_context)
    assert result == "1 + 2: test_session"


@pytest.mark.asyncio
async def test_compile_function_def_async_context(mock_context):
    code = """
async def myfunc(input, context):
    return f"{input.a} + {input.b}: {context.session_id}"
"""
    func = _compile_function(code, "myfunc", make_schema())
    result = await func({"a": 1, "b": 2}, context=mock_context)
    assert result == "1 + 2: test_session"


@pytest.mark.asyncio
async def test_compile_function_def_async(mock_context):
    code = """
async def myfunc(input):
    return input.a * 2
"""
    func = _compile_function(code, "myfunc", make_schema())
    result = await func({"a": 7, "b": 1}, context=mock_context)
    assert result == 14


@pytest.mark.asyncio
async def test_input_validation(mock_context):
    code = "input.a + input.b"
    func = _compile_function(code, None, make_schema())
    with pytest.raises(ValueError):
        await func({"a": 1}, context=mock_context)  # missing 'b'
    with pytest.raises(ValueError):
        await func({"a": "bad", "b": 2}, context=mock_context)  # wrong type


def test_input_wrapper_encoding():
    data = {"a": 1, "b": {"c": 2}}
    wrapper = _InputWrapper(data)
    encoded = msgspec.json.encode(wrapper)
    assert encoded == b'{"a":1,"b":{"c":2}}'
