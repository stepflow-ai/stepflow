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

import datetime

from stepflow_py.worker.task_handler import (
    proto_value_to_python,
    python_to_proto_value,
)


def test_python_to_proto_value_datetime():
    dt = datetime.datetime(2025, 6, 15, 10, 30, 0, tzinfo=datetime.timezone.utc)
    value = python_to_proto_value(dt)
    assert value.string_value == "2025-06-15T10:30:00+00:00"


def test_python_to_proto_value_date():
    d = datetime.date(2025, 6, 15)
    value = python_to_proto_value(d)
    assert value.string_value == "2025-06-15"


def test_python_to_proto_value_time():
    t = datetime.time(10, 30, 0)
    value = python_to_proto_value(t)
    assert value.string_value == "10:30:00"


def test_python_to_proto_value_datetime_nested():
    """Datetime values nested in dicts and lists are converted."""
    dt = datetime.datetime(2025, 1, 1, tzinfo=datetime.timezone.utc)
    obj = {"created": dt, "tags": [dt]}
    value = python_to_proto_value(obj)
    result = proto_value_to_python(value)
    assert result == {
        "created": "2025-01-01T00:00:00+00:00",
        "tags": ["2025-01-01T00:00:00+00:00"],
    }
