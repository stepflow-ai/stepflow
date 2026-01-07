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

# Multi-stage build for Stepflow runtime server
# Build context: repository root (stepflow/)

# Build stage
FROM rust:1.90-bookworm AS builder

WORKDIR /build

# Copy workspace files from stepflow-rs directory
COPY stepflow-rs/Cargo.toml stepflow-rs/Cargo.lock ./
COPY stepflow-rs/crates ./crates

# Build release binary
RUN cargo build --release --bin stepflow-server


# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y \
        ca-certificates \
        libssl3 \
        curl \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /build/target/release/stepflow-server /usr/local/bin/stepflow-server

# Create app user
RUN useradd -m -u 1000 stepflow

# Create directories for config and data
RUN mkdir -p /app/config /app/data && \
    chown -R stepflow:stepflow /app

USER stepflow
WORKDIR /app

# Expose API port
EXPOSE 7840

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:7840/api/v1/health || exit 1

# Default command
CMD ["stepflow-server", "--port", "7840", "--config", "/app/config/stepflow-config.yml"]
