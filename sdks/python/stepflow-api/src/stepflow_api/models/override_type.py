from enum import Enum


class OverrideType(str, Enum):
    JSON_PATCH = "json_patch"
    MERGE_PATCH = "merge_patch"

    def __str__(self) -> str:
        return str(self.value)
