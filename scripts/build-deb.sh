#!/bin/bash
# Build script for r2 .deb package
set -euo pipefail

echo "=== Building r2 ==="

# Build the project
echo "Building project..."
cargo build --release

# Check if cargo-deb is installed
if ! command -v cargo-deb &> /dev/null; then
    echo "Installing cargo-deb..."
    cargo install cargo-deb
fi

# Build .deb package
echo "Building .deb package..."
cargo deb --package r2-ui

echo "=== Build complete ==="
echo "Binary: target/release/r2"
echo "Debian package: target/debian/*.deb"
