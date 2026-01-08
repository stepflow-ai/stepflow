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

"""Tests for runtime utilities."""

import platform

from stepflow_runtime.utils import find_free_port, get_binary_name, is_port_in_use


class TestFindFreePort:
    def test_find_free_port_returns_int(self):
        port = find_free_port()
        assert isinstance(port, int)

    def test_find_free_port_returns_valid_range(self):
        port = find_free_port()
        assert 1024 <= port <= 65535

    def test_find_free_port_is_available(self):
        port = find_free_port()
        # Port should be free immediately after finding it
        assert not is_port_in_use(port)


class TestIsPortInUse:
    def test_unused_port_returns_false(self):
        port = find_free_port()
        assert not is_port_in_use(port)


class TestGetBinaryName:
    def test_binary_name_windows(self):
        if platform.system() == "Windows":
            assert get_binary_name() == "stepflow-worker.exe"
        else:
            assert get_binary_name() == "stepflow-worker"

    def test_binary_name_unix(self):
        if platform.system() != "Windows":
            assert get_binary_name() == "stepflow-worker"
