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

# System-level provisioning for Lima k3s VM
set -eux -o pipefail

# Update package list
apt-get update

# Install essential packages
apt-get install -y \
  curl \
  wget \
  git \
  jq \
  socat \
  conntrack \
  ipset \
  iptables \
  ca-certificates \
  gnupg \
  lsb-release

# Install Docker
# Add Docker's official GPG key
install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
chmod a+r /etc/apt/keyrings/docker.asc

# Add Docker repository
echo \
  "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu \
  $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | \
  tee /etc/apt/sources.list.d/docker.list > /dev/null

# Install Docker packages
apt-get update
apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

# Add lima user to docker group
usermod -aG docker lima

# Install k3s with specific configuration
# --disable=traefik: We'll use our own ingress (Pingora)
# --write-kubeconfig-mode=644: Make kubeconfig readable
# --tls-san: Allow access from Mac host
curl -sfL https://get.k3s.io | INSTALL_K3S_EXEC="--disable=traefik --write-kubeconfig-mode=644 --tls-san=127.0.0.1" sh -

# Wait for k3s to be ready
echo "Waiting for k3s to be ready..."
timeout 60 sh -c 'until kubectl get nodes | grep -q " Ready"; do sleep 2; done' || {
    echo "k3s failed to start properly"
    systemctl status k3s
    exit 1
}

# Enable k3s local registry (for local development)
# This allows us to push images without external registry
mkdir -p /etc/rancher/k3s
cat > /etc/rancher/k3s/registries.yaml <<'REGISTRY_EOF'
mirrors:
  "localhost:5000":
    endpoint:
      - "http://localhost:5000"
REGISTRY_EOF

# Install local Docker registry
docker pull registry:2
docker run -d -p 5000:5000 --restart=always --name registry registry:2 || true

# Restart k3s to pick up registry config
systemctl restart k3s

echo "✅ k3s cluster ready!"
echo "   Nodes: $(kubectl get nodes -o wide)"
echo "   k3s version: $(k3s --version | head -1)"
