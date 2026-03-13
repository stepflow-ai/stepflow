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

"""Tests for gRPC component registration and server behavior."""

from __future__ import annotations

from docling_proto_step_worker.converter_cache import ConverterCache

# The SDK's server.component() decorator auto-prepends "/" to names,
# and the shared server singleton includes a built-in "/udf" component.
# The gRPC worker strips "/" at lookup time (grpc_worker.py:292).
EXPECTED_DOCLING_COMPONENTS = {"/classify", "/convert", "/chunk"}


class TestServerRegistration:
    """Tests for component registration on the module-level server."""

    def test_registers_docling_components(self):
        from docling_proto_step_worker.server import server

        components = server.get_components()
        component_names = set(components.keys())

        for name in EXPECTED_DOCLING_COMPONENTS:
            assert name in component_names, f"Missing component: {name}"

    def test_docling_components_have_slash_prefix(self):
        """SDK auto-adds '/' prefix; gRPC worker strips it at lookup time."""
        from docling_proto_step_worker.server import server

        components = server.get_components()
        for name in EXPECTED_DOCLING_COMPONENTS:
            assert name in components

    def test_no_unexpected_docling_components(self):
        """Only docling components + SDK builtins (like /udf) should be registered."""
        from docling_proto_step_worker.server import server

        components = server.get_components()
        docling_components = {
            k for k in components if k.lstrip("/") in {"classify", "convert", "chunk"}
        }
        assert docling_components == EXPECTED_DOCLING_COMPONENTS

    def test_converter_cache_is_module_singleton(self):
        from docling_proto_step_worker.server import converter_cache

        assert isinstance(converter_cache, ConverterCache)
