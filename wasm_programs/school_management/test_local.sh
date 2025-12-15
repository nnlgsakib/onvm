#!/bin/bash

# Local testing script for school management WASM program

echo "Testing school management WASM program locally..."

# Check if onvm CLI is installed
if ! command -v onvm &> /dev/null
then
    echo "ONVM CLI is not installed."
    exit 1
fi

# Initialize a local node if it doesn't exist
if [ ! -d "./test_node" ]; then
    echo "Initializing test node..."
    onvm init --data-dir ./test_node
fi

# Start a local node in the background
echo "Starting local node..."
onvm run-node --data-dir ./test_node --listen /ip4/127.0.0.1/tcp/37001 --rpc 127.0.0.1:8081 &
NODE_PID=$!

# Wait a moment for the node to start
sleep 3

# Check if the node is running
if ps -p $NODE_PID > /dev/null
then
    echo "Local node started successfully (PID: $NODE_PID)"
else
    echo "Failed to start local node"
    exit 1
fi

# Deploy the program to the local node
echo "Deploying program to local node..."
DEPLOY_RESULT=$(onvm deploy --rpc 127.0.0.1:8081 --file school_management.wasm --entrypoint onvm_main 2>&1)
echo "$DEPLOY_RESULT"

# Extract program ID if deployment was successful
if echo "$DEPLOY_RESULT" | grep -q "id:"; then
    PROGRAM_ID=$(echo "$DEPLOY_RESULT" | grep -oE 'id: [a-f0-9]+' | cut -d' ' -f2)
    echo "Program deployed with ID: $PROGRAM_ID"
    
    # Test a simple operation
    echo "Testing add_student operation..."
    TEST_RESULT=$(onvm execute --rpc 127.0.0.1:8081 --program_id $PROGRAM_ID --input interactions/add_student.json 2>&1)
    echo "Test result:"
    echo "$TEST_RESULT"
else
    echo "Deployment failed:"
    echo "$DEPLOY_RESULT"
fi

# Stop the local node
echo "Stopping local node..."
kill $NODE_PID

echo "Local test completed."