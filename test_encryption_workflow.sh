#!/bin/bash

# Test script for image encryption workflow
# Tests the complete workflow: client -> leader -> worker -> encrypt -> return

set -e

echo "=========================================="
echo "  Image Encryption Workflow Test"
echo "=========================================="
echo ""

# Create test image if it doesn't exist
TEST_IMAGE="test_image.png"
if [ ! -f "$TEST_IMAGE" ]; then
    echo "Creating test image (100x100 PNG)..."
    # Create a simple PNG image using ImageMagick or fallback to a binary file
    if command -v convert &> /dev/null; then
        convert -size 100x100 xc:blue "$TEST_IMAGE"
    else
        # Create a simple PNG file manually (PNG signature + minimal data)
        printf '\x89\x50\x4e\x47\x0d\x0a\x1a\x0a' > "$TEST_IMAGE"
        dd if=/dev/urandom bs=1024 count=10 >> "$TEST_IMAGE" 2>/dev/null
    fi
    echo "✓ Test image created: $TEST_IMAGE ($(stat -c%s "$TEST_IMAGE") bytes)"
fi

# Clean up old data
echo ""
echo "Cleaning up old data..."
rm -rf shared_images encrypted_images
rm -f /tmp/node*.log
echo "✓ Cleanup complete"

# Build the project
echo ""
echo "Building project..."
cargo build --release 2>&1 | grep -E "(Compiling|Finished)" || true
echo "✓ Build complete"

# Start the 3 server nodes
echo ""
echo "=========================================="
echo "  Starting Server Nodes"
echo "=========================================="

CONFIG_FILE="./config.json"

echo ""
echo "Starting Node 1..."
NODE_ID=1 CONFIG_FILE="$CONFIG_FILE" ./target/release/cloud-p2p > /tmp/node1.log 2>&1 &
NODE1_PID=$!
echo "✓ Node 1 started (PID: $NODE1_PID)"

echo ""
echo "Starting Node 2..."
NODE_ID=2 CONFIG_FILE="$CONFIG_FILE" ./target/release/cloud-p2p > /tmp/node2.log 2>&1 &
NODE2_PID=$!
echo "✓ Node 2 started (PID: $NODE2_PID)"

echo ""
echo "Starting Node 3..."
NODE_ID=3 CONFIG_FILE="$CONFIG_FILE" ./target/release/cloud-p2p > /tmp/node3.log 2>&1 &
NODE3_PID=$!
echo "✓ Node 3 started (PID: $NODE3_PID)"

# Function to cleanup on exit
cleanup() {
    echo ""
    echo "=========================================="
    echo "  Cleaning Up"
    echo "=========================================="
    echo "Stopping all nodes..."
    kill $NODE1_PID $NODE2_PID $NODE3_PID 2>/dev/null || true
    echo "✓ All nodes stopped"
}

trap cleanup EXIT

# Wait for nodes to start and elect leader
echo ""
echo "Waiting 5 seconds for nodes to start and elect leader..."
sleep 5

# Show election results
echo ""
echo "=========================================="
echo "  Election Results"
echo "=========================================="
echo ""
echo "Node 1 logs (last 10 lines):"
tail -10 /tmp/node1.log | grep -E "(ELECTION|LEADER|HEARTBEAT)" || echo "  (no election logs yet)"
echo ""
echo "Node 2 logs (last 10 lines):"
tail -10 /tmp/node2.log | grep -E "(ELECTION|LEADER|HEARTBEAT)" || echo "  (no election logs yet)"
echo ""
echo "Node 3 logs (last 10 lines):"
tail -10 /tmp/node3.log | grep -E "(ELECTION|LEADER|HEARTBEAT)" || echo "  (no election logs yet)"

# Send test image using client
echo ""
echo "=========================================="
echo "  Sending Test Image"
echo "=========================================="
echo ""
echo "Starting client and sending $TEST_IMAGE..."
echo ""

# Use timeout and send image command (pass CONFIG_FILE explicitly)
(echo "send-image $TEST_IMAGE"; sleep 5) | timeout 10 env CONFIG_FILE="$CONFIG_FILE" ./target/release/client || true

# Wait a bit for processing
echo ""
echo "Waiting 3 seconds for image processing..."
sleep 3

# Check results
echo ""
echo "=========================================="
echo "  Results"
echo "=========================================="

# Check if encrypted image was saved
echo ""
echo "Checking client encrypted_images directory..."
if [ -d "encrypted_images" ]; then
    ENCRYPTED_COUNT=$(ls -1 encrypted_images/ 2>/dev/null | wc -l)
    if [ "$ENCRYPTED_COUNT" -gt 0 ]; then
        echo "✅ SUCCESS: Found $ENCRYPTED_COUNT encrypted image(s) in encrypted_images/"
        ls -lh encrypted_images/

        # Show size comparison
        ORIGINAL_SIZE=$(stat -c%s "$TEST_IMAGE")
        ENCRYPTED_FILE=$(ls encrypted_images/ | head -1)
        ENCRYPTED_SIZE=$(stat -c%s "encrypted_images/$ENCRYPTED_FILE")
        echo ""
        echo "Size comparison:"
        echo "  Original: $ORIGINAL_SIZE bytes"
        echo "  Encrypted: $ENCRYPTED_SIZE bytes"
        echo "  Overhead: $((ENCRYPTED_SIZE - ORIGINAL_SIZE)) bytes"
    else
        echo "✗ FAILED: No encrypted images found"
    fi
else
    echo "✗ FAILED: encrypted_images directory not created"
fi

# Check that servers did NOT store images
echo ""
echo "Checking that servers did NOT permanently store raw images..."
if [ -d "shared_images" ]; then
    IMAGE_COUNT=$(ls -1 shared_images/ 2>/dev/null | wc -l)
    if [ "$IMAGE_COUNT" -eq 0 ]; then
        echo "✅ SUCCESS: No images stored on servers (encryption-only service)"
    else
        echo "⚠️  WARNING: Found $IMAGE_COUNT image(s) in shared_images/ (should be 0)"
        echo "   This means servers are storing images, which is incorrect"
    fi
else
    echo "✅ SUCCESS: shared_images directory doesn't exist (no server storage)"
fi

# Show server logs for encryption workflow
echo ""
echo "=========================================="
echo "  Server Processing Logs"
echo "=========================================="

echo ""
echo "Node 1 encryption logs:"
grep -E "(RECEIVED EncryptRequest|BALANCER|ENCRYPTING|ENCRYPTED|Sent encrypted)" /tmp/node1.log | tail -20 || echo "  (no encryption activity)"

echo ""
echo "Node 2 encryption logs:"
grep -E "(RECEIVED EncryptRequest|BALANCER|ENCRYPTING|ENCRYPTED|Sent encrypted)" /tmp/node2.log | tail -20 || echo "  (no encryption activity)"

echo ""
echo "Node 3 encryption logs:"
grep -E "(RECEIVED EncryptRequest|BALANCER|ENCRYPTING|ENCRYPTED|Sent encrypted)" /tmp/node3.log | tail -20 || echo "  (no encryption activity)"

echo ""
echo "=========================================="
echo "  Test Complete"
echo "=========================================="
