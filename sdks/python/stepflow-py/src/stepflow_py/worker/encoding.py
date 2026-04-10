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

"""Default encoding hook for converting non-JSON types to JSON-compatible values.

This module provides the default ``enc_hook`` used by :class:`FlowBuilder` to
convert non-JSON-native types during ``_auto_convert_input``.  Users can supply
their own hook to handle additional types; the convention (matching *msgspec*)
is to raise :class:`TypeError` for unrecognised objects so callers can fall back.
"""

from __future__ import annotations

import datetime
from typing import Any


def default_enc_hook(obj: Any) -> Any:
    """Convert non-JSON-native Python objects to JSON-compatible values.

    Supported conversions:
    - ``datetime.datetime`` → ISO 8601 string (via ``.isoformat()``)
    - ``datetime.date`` → ISO 8601 string (via ``.isoformat()``)
    - ``datetime.time`` → ISO 8601 string (via ``.isoformat()``)

    Raises:
        TypeError: For types that are not handled.
    """
    # Note: check datetime before date, since datetime is a subclass of date.
    if isinstance(obj, datetime.datetime):
        return obj.isoformat()
    if isinstance(obj, datetime.date):
        return obj.isoformat()
    if isinstance(obj, datetime.time):
        return obj.isoformat()
    raise TypeError(f"Encoding objects of type {type(obj)} is not supported")
