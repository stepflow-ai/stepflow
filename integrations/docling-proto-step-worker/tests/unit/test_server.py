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

# Component IDs are bare identifiers (no leading slash).
# The subpath (with leading slash) is a separate concept used for routing.
# The SDK's built-in "udf" component is also registered by default.
EXPECTED_DOCLING_IDS = {"classify", "convert", "chunk"}


class TestServerRegistration:
    """Tests for component registration on the module-level server."""

    def test_registers_docling_components(self):
        """All docling components are registered by their component ID."""
        from docling_proto_step_worker.server import server

        components = server.get_components()
        component_ids = set(components.keys())

        for cid in EXPECTED_DOCLING_IDS:
            assert cid in component_ids, f"Missing component: {cid}"

    def test_component_ids_have_no_slash(self):
        """Component IDs are bare identifiers, not paths."""
        from docling_proto_step_worker.server import server

        components = server.get_components()
        for cid in EXPECTED_DOCLING_IDS:
            assert cid in components
            assert not cid.startswith("/"), (
                f"Component ID should not start with '/': {cid}"
            )

    def test_component_subpaths_have_slash(self):
        """Each component's subpath starts with '/' for mounting under a prefix."""
        from docling_proto_step_worker.server import server

        components = server.get_components()
        for cid in EXPECTED_DOCLING_IDS:
            entry = components[cid]
            assert entry.path.startswith("/"), (
                f"Subpath should start with '/': {entry.path}"
            )
            # Subpath should be "/{id}" by default
            assert entry.path == f"/{cid}"

    def test_no_unexpected_docling_components(self):
        """Only docling components + SDK builtins (like udf) should be registered."""
        from docling_proto_step_worker.server import server

        components = server.get_components()
        docling_components = {
            k for k in components if k in {"classify", "convert", "chunk"}
        }
        assert docling_components == EXPECTED_DOCLING_IDS

    def test_converter_cache_is_module_singleton(self):
        from docling_proto_step_worker.server import converter_cache

        assert isinstance(converter_cache, ConverterCache)
