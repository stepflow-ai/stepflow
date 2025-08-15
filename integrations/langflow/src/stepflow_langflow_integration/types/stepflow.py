"""Stepflow workflow and step type definitions."""

from dataclasses import dataclass, field
from typing import Dict, Any, List, Optional


@dataclass
class StepflowStep:
    """Represents a Stepflow workflow step."""
    
    id: str
    component: str
    input: Dict[str, Any]
    output_schema: Optional[Dict[str, Any]] = None
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert step to dictionary representation."""
        result = {
            "id": self.id,
            "component": self.component,
            "input": self.input,
        }
        
        if self.output_schema:
            result["output_schema"] = self.output_schema
        
        return result


@dataclass 
class StepflowWorkflow:
    """Represents a Stepflow workflow."""
    
    name: str
    steps: List[StepflowStep]
    description: Optional[str] = None
    version: str = "1.0"
    schema: str = "https://stepflow.org/schemas/v1/flow.json"
    output: Optional[Dict[str, Any]] = None
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert workflow to dictionary representation."""
        result = {
            "schema": self.schema,
            "name": self.name,
            "steps": [step.to_dict() for step in self.steps],
        }
        
        if self.description:
            result["description"] = self.description
        
        if self.version != "1.0":
            result["version"] = self.version
        
        if self.output:
            result["output"] = self.output
        
        return result