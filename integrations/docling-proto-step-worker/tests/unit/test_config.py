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

"""Tests for pipeline configuration."""

from docling.datamodel.pipeline_options import PdfPipelineOptions, TableFormerMode

from docling_proto_step_worker.config import PIPELINE_CONFIGS


class TestPipelineConfigs:
    def test_config_born_digital_disables_ocr(self):
        assert PIPELINE_CONFIGS["born_digital"].do_ocr is False

    def test_config_born_digital_disables_table_structure(self):
        assert PIPELINE_CONFIGS["born_digital"].do_table_structure is False

    def test_config_scanned_enables_ocr(self):
        assert PIPELINE_CONFIGS["scanned"].do_ocr is True

    def test_config_scanned_enables_table_structure(self):
        assert PIPELINE_CONFIGS["scanned"].do_table_structure is True

    def test_config_born_digital_with_tables_uses_accurate_tableformer(self):
        config = PIPELINE_CONFIGS["born_digital_with_tables"]
        assert config.do_table_structure is True
        assert config.table_structure_options.mode == TableFormerMode.ACCURATE

    def test_config_born_digital_with_tables_disables_ocr(self):
        assert PIPELINE_CONFIGS["born_digital_with_tables"].do_ocr is False

    def test_config_default_exists(self):
        assert "default" in PIPELINE_CONFIGS
        config = PIPELINE_CONFIGS["default"]
        assert isinstance(config, PdfPipelineOptions)

    def test_config_default_enables_table_structure(self):
        assert PIPELINE_CONFIGS["default"].do_table_structure is True

    def test_all_configs_are_pdf_pipeline_options(self):
        for name, config in PIPELINE_CONFIGS.items():
            assert isinstance(config, PdfPipelineOptions), (
                f"Config '{name}' is not PdfPipelineOptions"
            )
