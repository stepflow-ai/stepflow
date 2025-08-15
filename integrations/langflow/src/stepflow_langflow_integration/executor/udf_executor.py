"""UDF executor for Langflow components."""

import os
import sys
import asyncio
import inspect
from typing import Dict, Any, Optional, Type
from stepflow_py import StepflowContext

from .type_converter import TypeConverter
from ..utils.errors import ExecutionError


class UDFExecutor:
    """Executes Langflow components as UDFs with full compatibility."""
    
    def __init__(self):
        """Initialize UDF executor."""
        self.type_converter = TypeConverter()
    
    async def execute(self, input_data: Dict[str, Any], context: StepflowContext) -> Dict[str, Any]:
        """Execute a Langflow component UDF.
        
        Args:
            input_data: Component input containing blob_id and runtime inputs
            context: Stepflow context for blob operations
            
        Returns:
            Component execution result
        """
        try:
            # Get UDF blob data
            blob_id = input_data.get("blob_id")
            if not blob_id:
                raise ExecutionError("No blob_id provided")
            
            blob_data = await context.get_blob(blob_id)
            runtime_inputs = input_data.get("input", {})
            
            # Execute the component
            result = await self._execute_langflow_component(
                blob_data=blob_data,
                runtime_inputs=runtime_inputs,
            )
            
            # Serialize result for Stepflow
            serialized_result = self.type_converter.serialize_langflow_object(result)
            return {"result": serialized_result}
            
        except ExecutionError:
            raise
        except Exception as e:
            raise ExecutionError(f"UDF execution failed: {e}") from e
    
    async def _execute_langflow_component(
        self,
        blob_data: Dict[str, Any], 
        runtime_inputs: Dict[str, Any]
    ) -> Any:
        """Execute a Langflow component with proper class handling.
        
        Args:
            blob_data: UDF blob containing code and metadata
            runtime_inputs: Runtime inputs from other workflow steps
            
        Returns:
            Component execution result
        """
        # Extract UDF components
        code = blob_data.get("code", "")
        template = blob_data.get("template", {})
        component_type = blob_data.get("component_type", "")
        outputs = blob_data.get("outputs", [])
        selected_output = blob_data.get("selected_output")
        
        if not code:
            raise ExecutionError(f"No code found for component {component_type}")
        
        # Set up execution environment
        exec_globals = self._create_execution_environment()
        
        try:
            # Execute component code
            exec(code, exec_globals)
        except Exception as e:
            raise ExecutionError(f"Failed to execute component code: {e}")
        
        # Find component class
        component_class = self._find_component_class(exec_globals, component_type)
        if not component_class:
            raise ExecutionError(f"Component class {component_type} not found")
        
        # Instantiate component
        try:
            component_instance = component_class()
        except Exception as e:
            raise ExecutionError(f"Failed to instantiate {component_type}: {e}")
        
        # Configure component
        component_parameters = await self._prepare_component_parameters(
            template, runtime_inputs
        )
        
        # Use Langflow's configuration method
        if hasattr(component_instance, "set_attributes"):
            component_instance._parameters = component_parameters
            component_instance.set_attributes(component_parameters)
        
        # Execute component method
        execution_method = self._determine_execution_method(outputs, selected_output)
        if not execution_method:
            raise ExecutionError(f"No execution method found for {component_type}")
        
        if not hasattr(component_instance, execution_method):
            available = [m for m in dir(component_instance) if not m.startswith("_")]
            raise ExecutionError(
                f"Method {execution_method} not found in {component_type}. "
                f"Available: {available}"
            )
        
        try:
            method = getattr(component_instance, execution_method)
            
            if inspect.iscoroutinefunction(method):
                result = await method()
            else:
                # Handle sync methods safely
                result = await self._execute_sync_method_safely(method, component_type)
            
            return result
            
        except Exception as e:
            raise ExecutionError(f"Failed to execute {execution_method}: {e}")
    
    def _create_execution_environment(self) -> Dict[str, Any]:
        """Create safe execution environment with Langflow imports."""
        exec_globals = globals().copy()
        exec_globals["os"] = os
        exec_globals["sys"] = sys
        
        # Import common Langflow types
        try:
            from langflow.schema.message import Message
            from langflow.schema.data import Data
            from langflow.schema.dataframe import DataFrame
            from langflow.custom.custom_component.component import Component
            
            exec_globals.update({
                "Message": Message,
                "Data": Data,
                "DataFrame": DataFrame,
                "Component": Component,
            })
        except ImportError as e:
            raise ExecutionError(f"Failed to import Langflow components: {e}")
        
        return exec_globals
    
    def _find_component_class(
        self, 
        exec_globals: Dict[str, Any], 
        component_type: str
    ) -> Optional[Type]:
        """Find component class in execution environment."""
        component_class = exec_globals.get(component_type)
        if component_class and isinstance(component_class, type):
            return component_class
        
        # Search for class by name
        for name, obj in exec_globals.items():
            if (isinstance(obj, type) and 
                name.lower() == component_type.lower()):
                return obj
        
        return None
    
    async def _prepare_component_parameters(
        self, 
        template: Dict[str, Any], 
        runtime_inputs: Dict[str, Any]
    ) -> Dict[str, Any]:
        """Prepare component parameters from template and runtime inputs."""
        component_parameters = {}
        
        # Process template parameters
        for key, field_def in template.items():
            if isinstance(field_def, dict) and "value" in field_def:
                value = field_def["value"]
                
                # Handle environment variables
                env_var = self._determine_environment_variable(key, value, field_def)
                if env_var:
                    actual_value = os.getenv(env_var)
                    if actual_value:
                        value = actual_value
                    elif field_def.get("_input_type") == "SecretStrInput":
                        print(f"⚠️  Environment variable {env_var} not found for {key}", file=sys.stderr)
                
                component_parameters[key] = value
        
        # Add runtime inputs (these override template values)
        for key, value in runtime_inputs.items():
            # Convert Stepflow values back to Langflow types if needed
            if isinstance(value, dict) and "__langflow_type__" in value:
                actual_value = self.type_converter.deserialize_to_langflow_type(value)
                component_parameters[key] = actual_value
            else:
                component_parameters[key] = value
        
        return component_parameters
    
    def _determine_environment_variable(
        self, 
        field_name: str, 
        field_value: Any, 
        field_config: Dict[str, Any]
    ) -> Optional[str]:
        """Determine environment variable name for a field."""
        # Template string like "${OPENAI_API_KEY}"
        if (isinstance(field_value, str) and 
            field_value.startswith("${") and field_value.endswith("}")):
            return field_value[2:-1]
        
        # Direct env var name
        if (isinstance(field_value, str) and 
            field_value.isupper() and 
            "_" in field_value and 
            any(keyword in field_value for keyword in ["API_KEY", "TOKEN", "SECRET"])):
            return field_value
        
        # Secret input fields
        if field_config.get("_input_type") == "SecretStrInput":
            if field_name == "api_key":
                return "OPENAI_API_KEY"
            elif "openai" in field_name.lower():
                return "OPENAI_API_KEY"
            elif "anthropic" in field_name.lower():
                return "ANTHROPIC_API_KEY"
            else:
                return field_name.upper()
        
        return None
    
    def _determine_execution_method(
        self, 
        outputs: list, 
        selected_output: Optional[str]
    ) -> Optional[str]:
        """Determine execution method from outputs metadata."""
        if selected_output:
            for output in outputs:
                if output.get("name") == selected_output:
                    method = output.get("method")
                    if method:
                        return method
        
        # Fallback to first output's method
        if outputs:
            return outputs[0].get("method")
        
        return None
    
    async def _execute_sync_method_safely(self, method, component_type: str):
        """Execute sync method safely in async context."""
        # For problematic components, use thread pool
        if component_type in ["URLComponent", "RecursiveUrlLoader"]:
            import concurrent.futures
            loop = asyncio.get_event_loop()
            with concurrent.futures.ThreadPoolExecutor() as executor:
                future = executor.submit(method)
                return await loop.run_in_executor(None, lambda: future.result())
        else:
            return method()