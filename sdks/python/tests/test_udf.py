import pytest
import asyncio
from types import SimpleNamespace
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


def test_input_wrapper_access():
    data = {"a": 1, "b": {"c": 2}}
    wrapper = _InputWrapper(data)
    assert wrapper.a == 1
    assert wrapper["a"] == 1
    assert wrapper.b.c == 2
    assert wrapper["b"].c == 2
    with pytest.raises(Exception):
        _ = wrapper.x


@pytest.mark.asyncio
async def test_compile_lambda_body():
    code = "input.a + input.b"
    func = _compile_function(code, None, make_schema(), context=None)
    result = await func({"a": 2, "b": 3})
    assert result == 5


@pytest.mark.asyncio
async def test_compile_function_body():
    code = """
res = input.a * input.b
return res
"""
    func = _compile_function(code, None, make_schema(), context=None)
    result = await func({"a": 2, "b": 4})
    assert result == 8


@pytest.mark.asyncio
async def test_compile_function_def_sync():
    code = """
def myfunc(input):
    return input.a - input.b
"""
    func = _compile_function(code, "myfunc", make_schema(), context=None)
    result = await func({"a": 5, "b": 3})
    assert result == 2


@pytest.mark.asyncio
async def test_compile_function_def_sync_context():
    code = """
def myfunc(input, context):
    return input.a + input.b + getattr(context, 'extra', 0)
"""
    ctx = SimpleNamespace(extra=10)
    func = _compile_function(code, "myfunc", make_schema(), context=ctx)
    result = await func({"a": 1, "b": 2}, ctx)
    assert result == 13


@pytest.mark.asyncio
async def test_compile_function_def_async():
    code = """
async def myfunc(input):
    return input.a * 2
"""
    func = _compile_function(code, "myfunc", make_schema(), context=None)
    result = await func({"a": 7, "b": 1})
    assert result == 14


@pytest.mark.asyncio
async def test_compile_function_def_async_context():
    code = """
async def myfunc(input, context):
    return input.a + input.b + getattr(context, 'extra', 0)
"""
    ctx = SimpleNamespace(extra=5)
    func = _compile_function(code, "myfunc", make_schema(), context=ctx)
    result = await func({"a": 2, "b": 3}, ctx)
    assert result == 10


@pytest.mark.asyncio
async def test_input_validation():
    code = "input.a + input.b"
    func = _compile_function(code, None, make_schema(), context=None)
    with pytest.raises(ValueError):
        await func({"a": 1})  # missing 'b'
    with pytest.raises(ValueError):
        await func({"a": "bad", "b": 2})  # wrong type
