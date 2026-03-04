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

# Base on docling-serve which has docling + models pre-installed
FROM quay.io/docling-project/docling-serve-cpu:v1.14.0

USER 0

# Install local stepflow-py SDK with http extras (includes FastAPI instrumentation)
COPY sdks/python/stepflow-py/ /tmp/stepflow-py/
RUN pip install --no-cache-dir \
    "/tmp/stepflow-py[http]" \
    "click>=8.0" \
    "python-dotenv>=1.0.0" \
    "nest-asyncio>=1.6.0" && \
    rm -rf /tmp/stepflow-py

WORKDIR /app
COPY --chown=1001:0 integrations/docling-step-worker/src/ /app/src/
ENV PYTHONPATH=/app/src

USER 1001

EXPOSE 8080

ENTRYPOINT ["python", "-m", "docling_step_worker.cli.main", "serve", "--host", "0.0.0.0", "--port", "8080"]
