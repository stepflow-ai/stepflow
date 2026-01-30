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

"""Hash-to-module mappings for known Langflow core components.

This module contains mappings of code hashes to component module paths for
known Langflow core components. When a component's code_hash matches an entry
here and the module matches, we can use the core executor instead of compiling
custom code from a blob.

The hashes are derived from the Langflow workflow metadata and can be found in
the `metadata.code_hash` and `metadata.module` fields of each node.
"""

import logging
from dataclasses import dataclass

logger = logging.getLogger(__name__)


@dataclass
class KnownComponent:
    """A known core component mapping."""

    code_hash: str
    module: (
        str  # Full module path, e.g., "lfx.components.docling.DoclingInlineComponent"
    )
    description: str = ""


# Mapping of code_hash -> KnownComponent
# These hashes are version-specific and may need updating when Langflow versions change
KNOWN_COMPONENTS: dict[str, KnownComponent] = {
    # ChatInput component (lfx)
    "f701f686b325": KnownComponent(
        code_hash="f701f686b325",
        module="lfx.components.input_output.chat.ChatInput",
        description="Chat Input component",
    ),
    # ChatOutput component (lfx)
    "9647f4d2f4b4": KnownComponent(
        code_hash="9647f4d2f4b4",
        module="lfx.components.input_output.chat_output.ChatOutput",
        description="Chat Output component",
    ),
    # LanguageModelComponent (lfx)
    "bb5f8714781b": KnownComponent(
        code_hash="bb5f8714781b",
        module="lfx.components.models.language_model.LanguageModelComponent",
        description="Language Model component",
    ),
    # Note: PromptComponent (langflow.components.prompts.prompt) is NOT included
    # because the module doesn't exist in the lfx package - it must be compiled
    # from custom code.
    # Add more known components here as needed
    # Example for DoclingInlineComponent (hash TBD):
    # "HASH_HERE": KnownComponent(
    #     code_hash="HASH_HERE",
    #     module="lfx.components.docling.DoclingInlineComponent",
    #     description="Docling inline document processing",
    # ),
}


def lookup_known_component(
    code_hash: str, module: str | None = None
) -> KnownComponent | None:
    """Look up a known component by code hash.

    Args:
        code_hash: The code_hash from workflow metadata
        module: Optional module path to verify (safety check)

    Returns:
        KnownComponent if found and module matches (if provided), None otherwise
    """
    known = KNOWN_COMPONENTS.get(code_hash)

    if known is None:
        return None

    # If module provided, verify it matches
    if module is not None and known.module != module:
        # Hash collision or outdated mapping - log warning and return None
        logger.warning(
            f"Code hash {code_hash} matches known component {known.module} "
            f"but workflow specifies {module}. Using custom_code executor."
        )
        return None

    return known


def module_to_path(module: str) -> str:
    """Convert module path to URL path segment.

    Example: "lfx.components.docling.DoclingInlineComponent"
          -> "lfx/components/docling/DoclingInlineComponent"

    Args:
        module: Full module path with dots

    Returns:
        URL path segment with slashes
    """
    return module.replace(".", "/")
