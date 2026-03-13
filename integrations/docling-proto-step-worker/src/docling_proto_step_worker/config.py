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

"""Pipeline configurations for docling document processing.

Named pipeline configurations that the classify step maps to.
Each configuration creates a separate DocumentConverter instance
since pipeline options are fixed at construction time.
"""

from docling.datamodel.pipeline_options import (
    PdfPipelineOptions,
    TableFormerMode,
    TableStructureOptions,
)

PIPELINE_CONFIGS: dict[str, PdfPipelineOptions] = {
    "born_digital": PdfPipelineOptions(
        do_ocr=False,
        do_table_structure=False,
        generate_picture_images=True,  # match docling-serve default
    ),
    "born_digital_with_tables": PdfPipelineOptions(
        do_ocr=False,
        do_table_structure=True,
        generate_picture_images=True,  # match docling-serve default
        table_structure_options=TableStructureOptions(mode=TableFormerMode.ACCURATE),
    ),
    "scanned": PdfPipelineOptions(
        do_ocr=True,
        do_table_structure=True,
        generate_picture_images=True,  # match docling-serve default
        table_structure_options=TableStructureOptions(mode=TableFormerMode.FAST),
    ),
    "default": PdfPipelineOptions(
        do_table_structure=True,
        generate_picture_images=True,  # match docling-serve default
        table_structure_options=TableStructureOptions(mode=TableFormerMode.FAST),
    ),
}
