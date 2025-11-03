#!/bin/bash

# Simple manual test

set -e

# Kill any existing processes
pkill -9 -f "cloud-p2p" 2>/dev/null || true
pkill -9 -f "client" 2>/dev/null || true
sleep 1

# Clean up
rm -rf shared_images encrypted_images /tmp/node*.log

echo "Starting Node 1..."
export NODE_ID=1
export CONFIG_FILE=./config_localhost.json
./target/release/cloud-p2p > /tmp/node1.log 2>&1 &
NODE1_PID=$!

echo "Starting Node 2..."
export NODE_ID=2
export CONFIG_FILE=./config_localhost.json
./target/release/cloud-p2p > /tmp/node2.log 2>&1 &
NODE2_PID=$!

echo "Starting Node 3..."
export NODE_ID=3
export CONFIG_FILE=./config_localhost.json
./target/release/cloud-p2p > /tmp/node3.log 2>&1 &
NODE3_PID=$!

echo "Waiting for election to complete..."
sleep 5

echo ""
echo "===== ELECTION RESULTS ====="
grep "LEADER" /tmp/node*.log || echo "No leader elected yet"

echo ""
echo "===== SENDING TEST IMAGE ====="
export CONFIG_FILE=./config_localhost.json
(echo "send-image test_image.png"; sleep 3) | timeout 10 ./target/release/client

sleep 3

echo ""
echo "===== RESULTS ====="
echo "Encrypted images:"
ls -lh encrypted_images/ 2>/dev/null || echo "No encrypted images"

echo ""
echo "Server logs (encryption):"
grep -h "ENCRYPT" /tmp/node*.log | head -20 || echo "No encryption logs"

# Clean up
kill $NODE1_PID $NODE2_PID $NODE3_PID 2>/dev/null || true
