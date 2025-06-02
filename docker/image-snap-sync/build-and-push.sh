#!/bin/bash
set -e

# Check required env vars
if [[ -z "$DOCKER_USERNAME" || -z "$DOCKER_PASSWORD" ]]; then
  echo "DOCKER_USERNAME and DOCKER_PASSWORD must be set."
  exit 1
fi

# Ensure script runs from its own directory
cd "$(dirname "$0")"

# Login to GHCR
echo "$DOCKER_PASSWORD" | docker login ghcr.io -u "$DOCKER_USERNAME" --password-stdin

# Extract crate version from top-level Cargo.toml
SOFTWARE_VERSION=$(grep '^version' ../../Cargo.toml | head -n1 | cut -d '"' -f2)
SOFTWARE_IMAGE_NAME=frigate-snap-sync
IMAGE_NAME="ghcr.io/$DOCKER_USERNAME/$SOFTWARE_IMAGE_NAME"

# Build and push for arm64 and amd64
docker buildx build --platform linux/amd64,linux/arm64 --push -t ghcr.io/$DOCKER_USERNAME/$SOFTWARE_IMAGE_NAME:$SOFTWARE_VERSION .
docker buildx build --platform linux/amd64,linux/arm64 --push -t ghcr.io/$DOCKER_USERNAME/$SOFTWARE_IMAGE_NAME:latest .
