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

from typing import Any
from msgspec import Struct, Raw
from uuid import UUID


class RemoteError(Struct, kw_only=True):
    """
    The error that occurred during the method execution.
    """

    code: int
    """
    The error code.
    """

    message: str
    """
    The error message.
    """

    data: dict[str, Any] = {}
    """
    The error data.
    """

class Message(Struct, kw_only=True):
    """
    Message sent to request a method execution.
    """

    jsonrpc: str = "2.0"
    """
    The JSON-RPC version (must be "2.0")
    """

    id: UUID | None = None
    """
    The request id. If not set, this is a notification.
    """

    method: str | None = None
    """
    The method to execute.
    """

    params: Raw = Raw(b"null")
    """
    The parameters to pass to the method.
    """

    result: Raw = Raw(b"null")
    """
    The result of the method execution.
    """

    error: RemoteError | None = None
    """
    The error that occurred during the method execution.
    """