"""Custom exceptions for docling step worker."""


class DoclingStepWorkerError(Exception):
    """Base exception for docling step worker."""


class ClassificationError(DoclingStepWorkerError):
    """Error during document classification."""


class ConversionError(DoclingStepWorkerError):
    """Error during document conversion."""


class ChunkingError(DoclingStepWorkerError):
    """Error during document chunking."""


class BlobStoreError(DoclingStepWorkerError):
    """Error accessing blob store."""
