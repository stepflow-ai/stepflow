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

import msgspec
import pytest
from pytest_mock import MockerFixture

from stepflow_py.context import StepflowContext
from stepflow_py.exceptions import SkipStep, StepflowValueError
from stepflow_py.udf import UdfCompilationError, _compile_function, _InputWrapper


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

    func = _compile_function(code, make_schema())
    result = await func({"a": 2, "b": 3}, context=mock_context)
    assert result == 5


@pytest.mark.asyncio
async def test_compile_function_body(mock_context):
    code = """
res = input.a * input.b
return res
"""
    func = _compile_function(code, make_schema())
    result = await func({"a": 2, "b": 4}, context=mock_context)
    assert result == 8


@pytest.mark.asyncio
async def test_compile_function_def_sync(mock_context):
    code = """
def myfunc(input):
    return input.a - input.b
myfunc
"""
    func = _compile_function(code, make_schema())
    result = await func({"a": 5, "b": 3}, context=mock_context)
    assert result == 2


@pytest.mark.asyncio
async def test_compile_function_def_sync_context(mock_context):
    code = """
def myfunc(input, context):
    return f"{input.a} + {input.b}: {context.session_id}"
myfunc
"""
    func = _compile_function(code, make_schema())
    result = await func({"a": 1, "b": 2}, context=mock_context)
    assert result == "1 + 2: test_session"


@pytest.mark.asyncio
async def test_compile_function_def_async_context(mock_context):
    code = """
async def myfunc(input, context):
    return f"{input.a} + {input.b}: {context.session_id}"
myfunc
"""
    func = _compile_function(code, make_schema())
    result = await func({"a": 1, "b": 2}, context=mock_context)
    assert result == "1 + 2: test_session"


@pytest.mark.asyncio
async def test_compile_function_def_async(mock_context):
    code = """
async def myfunc(input):
    return input.a * 2
myfunc
"""
    func = _compile_function(code, make_schema())
    result = await func({"a": 7, "b": 1}, context=mock_context)
    assert result == 14


@pytest.mark.asyncio
async def test_input_validation(mock_context):
    code = "input.a + input.b"
    func = _compile_function(code, make_schema())
    with pytest.raises(ValueError):
        await func({"a": 1}, context=mock_context)  # missing 'b'
    with pytest.raises(ValueError):
        await func({"a": "bad", "b": 2}, context=mock_context)  # wrong type


def test_input_wrapper_encoding():
    data = {"a": 1, "b": {"c": 2}}
    wrapper = _InputWrapper(data)
    encoded = msgspec.json.encode(wrapper)
    assert encoded == b'{"a":1,"b":{"c":2}}'


# LangChain-specific tests
@pytest.mark.asyncio
async def test_langchain_runnable_return_pattern(mock_context):
    """Test UDF with LangChain runnable using return statement (preferred pattern)."""
    pytest.importorskip("langchain_core")

    # This uses the new cleaner return pattern
    code = '''
from langchain_core.runnables import RunnableLambda

def process_text(data):
    """Custom text processing function created via UDF."""
    text = data["text"]
    words = text.split()

    # Custom processing logic
    processed_words = []
    for word in words:
        if len(word) > 3:
            processed_words.append(word.upper())
        else:
            processed_words.append(word.lower())

    return {
        "processed_text": " ".join(processed_words),
        "word_count": len(words),
        "long_word_count": sum(1 for w in words if len(w) > 3),
        "short_word_count": sum(1 for w in words if len(w) <= 3),
        "processed_by": "custom_udf_processor",
        "original_length": len(text)
    }

# UDF returns the runnable directly - simpler and cleaner
return RunnableLambda(process_text)
'''

    schema = {
        "type": "object",
        "properties": {"text": {"type": "string"}},
        "required": ["text"],
    }

    func = _compile_function(code, schema)
    test_input = {
        "text": "This demonstrates self-contained user-defined LangChain runnables!"
    }
    result = await func(test_input, context=mock_context)

    # The result should not be None
    assert result is not None

    # Check expected structure
    assert isinstance(result, dict)
    assert "processed_text" in result
    assert "word_count" in result
    assert result["word_count"] == 6
    assert result["processed_by"] == "custom_udf_processor"


@pytest.mark.asyncio
async def test_langchain_runnable_lambda_expression(mock_context):
    """Test UDF with LangChain runnable using lambda expression pattern."""
    pytest.importorskip("langchain_core")

    # This tests a simple lambda expression pattern
    code = (
        "RunnableLambda(lambda data: {"
        "'simple_result': f'Lambda processed: {data[\"text\"]}'})"
    )

    schema = {
        "type": "object",
        "properties": {"text": {"type": "string"}},
        "required": ["text"],
    }

    func = _compile_function(code, schema)
    test_input = {"text": "This is a lambda test!"}
    result = await func(test_input, context=mock_context)

    # The result should not be None
    assert result is not None

    # Check expected structure
    assert isinstance(result, dict)
    assert "simple_result" in result
    assert result["simple_result"] == "Lambda processed: This is a lambda test!"


@pytest.mark.asyncio
async def test_simple_langchain_runnable_direct_invoke(mock_context):
    """Test direct LangChain runnable invocation to isolate the issue."""
    pytest.importorskip("langchain_core")

    from langchain_core.runnables import RunnableLambda

    def simple_processor(data):
        return {"result": f"Processed: {data['text']}"}

    # Test direct runnable invocation
    runnable = RunnableLambda(simple_processor)
    test_input = {"text": "hello world"}

    # This should work
    direct_result = runnable.invoke(test_input)
    assert direct_result == {"result": "Processed: hello world"}

    # Now test through UDF compilation using return pattern
    code = """
from langchain_core.runnables import RunnableLambda

def simple_processor(data):
    return {"result": f"Processed: {data['text']}"}

return RunnableLambda(simple_processor)
"""

    schema = {
        "type": "object",
        "properties": {"text": {"type": "string"}},
        "required": ["text"],
    }

    func = _compile_function(code, schema)
    udf_result = await func(test_input, context=mock_context)

    # This should match the direct invocation result
    assert udf_result == {"result": "Processed: hello world"}


@pytest.mark.asyncio
async def test_udf_pattern_search_like_integration_test(mock_context):
    """Test UDF compilation that matches the failing integration test pattern."""
    # This reproduces the pattern_search function from udf_text_processing.yaml
    code = """
text = input['text']
pattern = input['pattern']

try:
    matches = re.findall(pattern, text)
    return [{'match': match, 'index': i} for i, match in enumerate(matches)]
except:
    return []
"""

    schema = {
        "type": "object",
        "properties": {"text": {"type": "string"}, "pattern": {"type": "string"}},
        "required": ["text", "pattern"],
    }

    # This should compile successfully since it has a return statement
    func = _compile_function(code, schema)

    test_input = {
        "text": "The weather today is terrible and I hate it",
        "pattern": r"\b(weather|today|terrible|hate|awful)\b",
    }

    result = await func(test_input, context=mock_context)

    # Verify the result structure
    assert isinstance(result, list)
    assert len(result) == 4  # weather, today, terrible, hate
    assert result[0] == {"match": "weather", "index": 0}
    assert result[1] == {"match": "today", "index": 1}


@pytest.mark.asyncio
async def test_udf_word_analysis_like_integration_test(mock_context):
    """
    Test UDF compilation that matches the word_analysis function from integration test.
    """
    # This reproduces the word_analysis function from udf_text_processing.yaml
    code = """
text = input['text'].lower()
words = text.split()

word_count = len(words)
char_count = len(text.replace(' ', ''))

# Count word lengths
word_lengths = {}
for word in words:
    length = len(word)
    word_lengths[length] = word_lengths.get(length, 0) + 1

# Find most common words
word_freq = {}
for word in words:
    word_freq[word] = word_freq.get(word, 0) + 1

most_common = sorted(word_freq.items(), key=lambda x: x[1], reverse=True)[:3]

return {
    'word_count': word_count,
    'char_count': char_count,
    'avg_word_length': round(char_count / word_count, 2) if word_count > 0 else 0,
    'word_length_distribution': word_lengths,
    'most_common_words': [{'word': word, 'count': count} for word, count in most_common]
}
"""

    schema = {
        "type": "object",
        "properties": {"text": {"type": "string"}},
        "required": ["text"],
    }

    # This should compile successfully
    func = _compile_function(code, schema)

    test_input = {"text": "This is a great example"}

    result = await func(test_input, context=mock_context)

    # Verify the result structure
    assert isinstance(result, dict)
    assert "word_count" in result
    assert "char_count" in result
    assert "avg_word_length" in result
    assert result["word_count"] == 5


@pytest.mark.asyncio
async def test_udf_compilation_error_with_invalid_code(mock_context):
    """Test that UdfCompilationError is raised with proper context for invalid code."""
    # This code doesn't contain return statements and isn't a valid lambda expression
    invalid_code = """
text = input['text']
words = text.split()
word_count = len(words)
# Missing return statement - should fail compilation
"""

    schema = {
        "type": "object",
        "properties": {"text": {"type": "string"}},
        "required": ["text"],
    }

    with pytest.raises(UdfCompilationError) as exc_info:
        _compile_function(invalid_code, schema)

    # Check that the error contains the expected message
    error = exc_info.value
    assert "Unable to compile code" in str(error)
    assert error.data["code"] == invalid_code  # Code is stored in data dict now
    assert error.blob_id is None  # No blob_id when called directly


@pytest.mark.asyncio
async def test_skip_step_exception_propagation(mock_context):
    """Test that SkipStep exceptions are properly propagated through UDF execution."""
    # Simple lambda expression that raises SkipStep
    code = "raise SkipStep('Test skip condition')"

    schema = {
        "type": "object",
        "properties": {"test": {"type": "string"}},
        "required": ["test"],
    }

    func = _compile_function(code, schema)
    test_input = {"test": "value"}

    with pytest.raises(SkipStep) as exc_info:
        await func(test_input, context=mock_context)

    # Verify the exception has the expected message
    assert str(exc_info.value) == "Test skip condition"
    assert exc_info.value.message == "Test skip condition"


@pytest.mark.asyncio
async def test_skip_step_in_function_body(mock_context):
    """Test that SkipStep can be raised from function body code."""
    code = """
if input.should_skip:
    raise SkipStep("Skipping due to input condition")
return input.value * 2
"""

    schema = {
        "type": "object",
        "properties": {
            "should_skip": {"type": "boolean"},
            "value": {"type": "integer"},
        },
        "required": ["should_skip", "value"],
    }

    func = _compile_function(code, schema)

    # Test skip condition
    skip_input = {"should_skip": True, "value": 5}
    with pytest.raises(SkipStep) as exc_info:
        await func(skip_input, context=mock_context)
    assert str(exc_info.value) == "Skipping due to input condition"

    # Test normal execution
    normal_input = {"should_skip": False, "value": 5}
    result = await func(normal_input, context=mock_context)
    assert result == 10


@pytest.mark.asyncio
async def test_skip_step_in_sync_function(mock_context):
    """Test that SkipStep works with sync function definitions."""
    code = """
def my_func(input, context):
    if input.skip_me:
        raise SkipStep(f"Skipping for session {context.session_id}")
    return input.data
my_func
"""

    schema = {
        "type": "object",
        "properties": {"skip_me": {"type": "boolean"}, "data": {"type": "string"}},
        "required": ["skip_me", "data"],
    }

    func = _compile_function(code, schema)

    # Test skip condition
    skip_input = {"skip_me": True, "data": "test"}
    with pytest.raises(SkipStep) as exc_info:
        await func(skip_input, context=mock_context)
    assert "test_session" in str(exc_info.value)

    # Test normal execution
    normal_input = {"skip_me": False, "data": "test"}
    result = await func(normal_input, context=mock_context)
    assert result == "test"


@pytest.mark.asyncio
async def test_skip_step_in_async_function(mock_context):
    """Test that SkipStep works with async function definitions."""
    code = """
async def my_func(input):
    if input.condition == "skip":
        raise SkipStep("Async function skip")
    return {"processed": input.condition}
my_func
"""

    schema = {
        "type": "object",
        "properties": {"condition": {"type": "string"}},
        "required": ["condition"],
    }

    func = _compile_function(code, schema)

    # Test skip condition
    skip_input = {"condition": "skip"}
    with pytest.raises(SkipStep) as exc_info:
        await func(skip_input, context=mock_context)
    assert str(exc_info.value) == "Async function skip"

    # Test normal execution
    normal_input = {"condition": "continue"}
    result = await func(normal_input, context=mock_context)
    assert result == {"processed": "continue"}


@pytest.mark.asyncio
async def test_skip_step_with_langchain_runnable(mock_context):
    """Test that SkipStep works with LangChain runnable patterns."""
    pytest.importorskip("langchain_core")

    code = '''
from langchain_core.runnables import RunnableLambda

def conditional_processor(data):
    """Process data or skip based on condition."""
    if data.get("should_skip", False):
        raise SkipStep("Runnable requested skip")

    return {"result": f"Processed: {data['text']}"}

return RunnableLambda(conditional_processor)
'''

    schema = {
        "type": "object",
        "properties": {"text": {"type": "string"}, "should_skip": {"type": "boolean"}},
        "required": ["text"],
    }

    func = _compile_function(code, schema)

    # Test skip condition
    skip_input = {"text": "hello", "should_skip": True}
    with pytest.raises(SkipStep) as exc_info:
        await func(skip_input, context=mock_context)
    assert str(exc_info.value) == "Runnable requested skip"

    # Test normal execution
    normal_input = {"text": "hello", "should_skip": False}
    result = await func(normal_input, context=mock_context)
    assert result == {"result": "Processed: hello"}


@pytest.mark.asyncio
async def test_udf_compilation_error_includes_blob_context():
    """
    Test that UdfCompilationError includes blob_id context when called through udf().
    """
    from stepflow_py.udf import UdfInput, udf

    # Create a mock context
    async def mock_get_blob(self, blob_id):
        return {
            "code": "invalid code without return statement",
            "input_schema": {"type": "object"},
        }

    mock_context = type("MockContext", (), {"get_blob": mock_get_blob})()

    test_blob_id = "test_blob_123"
    udf_input = UdfInput(blob_id=test_blob_id, input={})

    with pytest.raises(UdfCompilationError) as exc_info:
        await udf(udf_input, mock_context)

    # Check that blob_id context is properly added
    error = exc_info.value
    assert error.blob_id == test_blob_id
    assert error.data["blob_id"] == test_blob_id
    assert f"Blob '{test_blob_id}'" in str(error)


@pytest.mark.asyncio
async def test_skip_step_through_udf_function():
    """Test that SkipStep propagates correctly through the full udf() function."""
    from stepflow_py.udf import UdfInput, udf

    # Create a mock context that returns blob with skip code
    async def mock_get_blob(self, blob_id):
        return {
            "code": "raise SkipStep('UDF function skip test')",
            "input_schema": {
                "type": "object",
                "properties": {"test": {"type": "string"}},
                "required": ["test"],
            },
        }

    mock_context = type("MockContext", (), {"get_blob": mock_get_blob})()

    test_blob_id = "test_skip_blob_123"
    udf_input = UdfInput(blob_id=test_blob_id, input={"test": "value"})

    with pytest.raises(SkipStep) as exc_info:
        await udf(udf_input, mock_context)

    # Verify the exception propagated correctly
    assert str(exc_info.value) == "UDF function skip test"
    assert exc_info.value.message == "UDF function skip test"
