#!/bin/bash
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

# User-level provisioning for Lima k3s VM
set -eux -o pipefail

# Copy kubeconfig to user directory for easy access
mkdir -p ~/.kube
sudo cp /etc/rancher/k3s/k3s.yaml ~/.kube/config
sudo chown $(id -u):$(id -g) ~/.kube/config
chmod 600 ~/.kube/config

# Add kubectl alias for convenience
echo 'export KUBECONFIG=~/.kube/config' >> ~/.bashrc
echo 'alias k=kubectl' >> ~/.bashrc

echo "k3s development environment ready!"
echo "Cluster info: $(kubectl cluster-info)"
echo ""
echo "Stepflow project mounted at: /home/lima.linux/stepflow"
