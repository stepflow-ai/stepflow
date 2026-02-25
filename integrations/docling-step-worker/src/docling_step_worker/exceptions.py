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
