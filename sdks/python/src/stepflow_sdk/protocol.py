from msgspec import Struct, Raw, field
from typing import Any, Union, TypeAlias
import msgspec

class InitializeRequest(Struct, kw_only=True):
    runtime_protocol_version: int

class InitializeResponse(Struct, kw_only=True):
    server_protocol_version: int

class Initialized(Struct, kw_only=True):
    pass

class ListComponentsRequest(Struct, kw_only=True):
    pass

class ListComponentsResponse(Struct, kw_only=True):
    components: list[str]

class ComponentInfoRequest(Struct, kw_only=True):
    component: str
    input_schema: dict[str, Any] = field(default_factory=dict)

class ComponentInfoResponse(Struct, kw_only=True):
    input_schema: dict[str, Any]
    output_schema: dict[str, Any]

class ComponentExecuteRequest(Struct, kw_only=True):
    component: str
    input: msgspec.Raw

class ComponentExecuteResponse(Struct, kw_only=True):
    output: msgspec.Raw