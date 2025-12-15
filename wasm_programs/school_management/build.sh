#!/bin/bash

# Build script for school management WASM program

echo "Building school management WASM program..."

# Check if tinygo is installed
if ! command -v tinygo &> /dev/null
then
    echo "TinyGo is not installed. Please install TinyGo to build this program."
    echo "Visit: https://tinygo.org/getting-started/"
    exit 1
fi

# Build the WASM module
tinygo build -o school_management.wasm -target wasi main.go

if [ $? -eq 0 ]; then
    echo "Build successful! school_management.wasm created."
    echo "You can now deploy this program to ONVM:"
    echo "  onvm deploy --file school_management.wasm --entrypoint onvm_main"
else
    echo "Build failed!"
    exit 1
fi