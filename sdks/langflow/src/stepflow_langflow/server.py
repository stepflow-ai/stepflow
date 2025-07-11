# Licensed to the Apache Software Foundation (ASF) under one or more contributor
# license agreements.  See the NOTICE file distributed with this work for
# additional information regarding copyright ownership.  The ASF licenses this
# file to you under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License.  You may obtain a copy of
# the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
# License for the specific language governing permissions and limitations under
# the License.

# Re-export the server from the base SDK
from stepflow_sdk.server import StepflowStdioServer
from stepflow_sdk.udf import udf
from stepflow_langflow.langflow_udf import langflow_udf

class StepflowLangflowServer(StepflowStdioServer):
    """Langflow-specific server that automatically registers langflow components."""
    
    def __init__(self, default_protocol_prefix: str = "langflow"):
        super().__init__(default_protocol_prefix)
        
        # Register the standard UDF component
        self.component(udf)
        
        # Register the Langflow UDF component
        self.component(langflow_udf)

__all__ = ["StepflowLangflowServer"]