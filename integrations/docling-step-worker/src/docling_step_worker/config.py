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
    ),
    "born_digital_with_tables": PdfPipelineOptions(
        do_ocr=False,
        do_table_structure=True,
        table_structure_options=TableStructureOptions(mode=TableFormerMode.ACCURATE),
    ),
    "scanned": PdfPipelineOptions(
        do_ocr=True,
        do_table_structure=True,
        table_structure_options=TableStructureOptions(mode=TableFormerMode.FAST),
    ),
    "default": PdfPipelineOptions(
        do_table_structure=True,
        table_structure_options=TableStructureOptions(mode=TableFormerMode.FAST),
    ),
}
