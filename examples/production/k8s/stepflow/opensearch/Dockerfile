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

FROM opensearchproject/opensearch:3.0.0

USER opensearch

# Remove default KNN and neural-search plugins
RUN opensearch-plugin remove opensearch-neural-search
RUN opensearch-plugin remove opensearch-knn

# Install jvector plugin for vector search
RUN echo y | opensearch-plugin install https://repo1.maven.org/maven2/org/opensearch/plugin/opensearch-jvector-plugin/3.0.0.3/opensearch-jvector-plugin-3.0.0.3.zip
