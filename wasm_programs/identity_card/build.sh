#!/bin/bash
set -e

echo "Building ONVM Identity Card WASM Program..."

cd "$(dirname "$0")"

if ! command -v rustup &> /dev/null; then
    echo "Error: rustup not found. Please install Rust first."
    exit 1
fi

echo "Adding wasm32-unknown-unknown target..."
rustup target add wasm32-unknown-unknown

echo "Building in release mode..."
cargo build --target wasm32-unknown-unknown --release

WASM_FILE="target/wasm32-unknown-unknown/release/onvm_identity_card.wasm"

if [ -f "$WASM_FILE" ]; then
    SIZE=$(wc -c < "$WASM_FILE")
    echo "✓ Build successful!"
    echo "  Output: $WASM_FILE"
    echo "  Size: $SIZE bytes"
else
    echo "✗ Build failed - WASM file not found"
    exit 1
fi

echo ""
echo "Next steps:"
echo "1. Upload: onvm upload-blob --file $WASM_FILE --rpc 127.0.0.1:8080"
echo "2. Deploy: onvm deploy --blob-id <BLOB_ID> --rpc 127.0.0.1:8080"
echo "3. Execute: onvm execute --program-id <PROGRAM_ID> --input register.json --rpc 127.0.0.1:8080"
