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

import pytest

from stepflow_py.worker.encoding import default_enc_hook


def test_enc_hook_datetime():
    dt = datetime.datetime(2025, 6, 15, 10, 30, 0, tzinfo=datetime.timezone.utc)
    assert default_enc_hook(dt) == "2025-06-15T10:30:00+00:00"


def test_enc_hook_datetime_naive():
    dt = datetime.datetime(2025, 6, 15, 10, 30, 0)
    assert default_enc_hook(dt) == "2025-06-15T10:30:00"


def test_enc_hook_date():
    d = datetime.date(2025, 6, 15)
    assert default_enc_hook(d) == "2025-06-15"


def test_enc_hook_time():
    t = datetime.time(10, 30, 0)
    assert default_enc_hook(t) == "10:30:00"


def test_enc_hook_time_with_tz():
    t = datetime.time(10, 30, 0, tzinfo=datetime.timezone.utc)
    assert default_enc_hook(t) == "10:30:00+00:00"


def test_enc_hook_unknown_type_raises():
    with pytest.raises(TypeError, match="not supported"):
        default_enc_hook(object())
