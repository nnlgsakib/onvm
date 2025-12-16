#!/bin/bash

# Build script for watermark_remover WASM module
echo "Building watermark_remover for WASM..."

# Check if wasm32 target is installed
if ! rustup target list --installed | grep -q "wasm32-unknown-unknown"; then
    echo "Installing wasm32-unknown-unknown target..."
    rustup target add wasm32-unknown-unknown
fi

# Build the project
echo "Compiling to WASM..."
cargo build --target wasm32-unknown-unknown --release

if [ $? -eq 0 ]; then
    echo "✅ Build successful!"
    echo "WASM file location: target/wasm32-unknown-unknown/release/watermark_remover.wasm"
    
    # Get file size
    SIZE=$(wc -c < target/wasm32-unknown-unknown/release/watermark_remover.wasm)
    echo "WASM file size: $SIZE bytes"
    
    if [ $SIZE -gt 1048576 ]; then
        echo "⚠️  Warning: WASM file is larger than 1MB, may need optimization"
    fi
else
    echo "❌ Build failed!"
    exit 1
fi