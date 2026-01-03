#!/bin/bash

# Deployment script for school management WASM program

echo "Deploying school management WASM program to ONVM..."

# Check if onvm CLI is installed
if ! command -v onvm &> /dev/null
then
    echo "ONVM CLI is not installed. Please install ONVM CLI to deploy this program."
    exit 1
fi

# Check if the WASM file exists
if [ ! -f "school_management.wasm" ]; then
    echo "school_management.wasm not found. Please build the program first."
    echo "Run: ./build.sh"
    exit 1
fi

# Deploy the program
echo "Deploying program..."
DEPLOY_OUTPUT=$(onvm deploy --file school_management.wasm --entrypoint onvm_main 2>&1)
DEPLOY_RESULT=$?

if [ $DEPLOY_RESULT -eq 0 ]; then
    echo "Deployment successful!"
    echo "$DEPLOY_OUTPUT"
    
    # Extract program ID from output (prefers prog<64-hex>, falls back to legacy 64-hex)
    PROGRAM_ID=$(echo "$DEPLOY_OUTPUT" | grep -oE 'prog[a-f0-9]{64}|[a-f0-9]{64}' | head -n1)
    
    if [ ! -z "$PROGRAM_ID" ]; then
        echo "Program ID: $PROGRAM_ID"
        echo "You can now execute the program using:"
        echo "  onvm execute --program_id $PROGRAM_ID --input test_requests.json"
    fi
else
    echo "Deployment failed!"
    echo "$DEPLOY_OUTPUT"
    exit 1
fi
