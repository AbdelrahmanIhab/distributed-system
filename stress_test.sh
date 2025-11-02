#!/bin/bash

# Stress Test for Distributed Load Balancer
# Tests concurrent client load and verifies distribution

set -e

echo "========================================"
echo "  LOAD BALANCER STRESS TEST"
echo "========================================"
echo ""

# Configuration
NUM_CLIENTS=10
REQUESTS_PER_CLIENT=10
TOTAL_REQUESTS=$((NUM_CLIENTS * REQUESTS_PER_CLIENT))
TEST_IMAGE="test_image.bin"
LOG_FILE="stress_test_$(date +%s).log"

# Create a test image if it doesn't exist
if [ ! -f "$TEST_IMAGE" ]; then
    echo "Creating test image ($TEST_IMAGE)..."
    dd if=/dev/urandom of="$TEST_IMAGE" bs=1024 count=100 2>/dev/null
    echo "Test image created (100KB)"
fi

echo "Test Configuration:"
echo "  - Concurrent Clients: $NUM_CLIENTS"
echo "  - Requests per Client: $REQUESTS_PER_CLIENT"
echo "  - Total Requests: $TOTAL_REQUESTS"
echo "  - Test Image: $TEST_IMAGE"
echo ""

# Check if servers are running
echo "Checking if servers are running..."
if ! pgrep -f "cloud-p2p" > /dev/null; then
    echo "ERROR: No server processes found!"
    echo "Please start the 3 server nodes first:"
    echo "  Terminal 1: SHARED_DIR=./shared_images NODE_ID=1 cargo run --release --bin cloud-p2p"
    echo "  Terminal 2: SHARED_DIR=./shared_images NODE_ID=2 cargo run --release --bin cloud-p2p"
    echo "  Terminal 3: SHARED_DIR=./shared_images NODE_ID=3 cargo run --release --bin cloud-p2p"
    exit 1
fi

echo "Servers detected. Waiting 2 seconds for leader election..."
sleep 2

echo ""
echo "========================================"
echo "  STARTING STRESS TEST"
echo "========================================"
echo ""

START_TIME=$(date +%s)

# Launch concurrent clients
echo "Launching $NUM_CLIENTS concurrent clients..."
for client_id in $(seq 1 $NUM_CLIENTS); do
    (
        for req_id in $(seq 1 $REQUESTS_PER_CLIENT); do
            echo "send-image $TEST_IMAGE" | cargo run --release --bin client 2>&1 | \
                grep -E "(LEADER|WORKER|RECORDED|SELECTED)" || true
        done
    ) >> "$LOG_FILE" 2>&1 &

    # Show progress
    echo "  Client $client_id launched"
done

echo ""
echo "All clients launched. Waiting for completion..."
wait

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

echo ""
echo "========================================"
echo "  TEST COMPLETED"
echo "========================================"
echo ""
echo "Duration: ${DURATION}s"
echo "Throughput: $((TOTAL_REQUESTS / DURATION)) requests/second"
echo ""

# Analyze results
echo "========================================"
echo "  LOAD DISTRIBUTION ANALYSIS"
echo "========================================"
echo ""

# Extract load counts from log
echo "Final load distribution:"
grep "RECORDED: Node" "$LOG_FILE" | tail -3 | awk '{print "  " $0}'

echo ""
echo "Distribution breakdown:"
for node in 1 2 3; do
    count=$(grep "RECORDED: Node $node" "$LOG_FILE" | tail -1 | grep -oP 'now has \K\d+' || echo "0")
    echo "  Node $node: $count requests"
done

echo ""

# Calculate variance
NODE1=$(grep "RECORDED: Node 1" "$LOG_FILE" | tail -1 | grep -oP 'now has \K\d+' || echo "0")
NODE2=$(grep "RECORDED: Node 2" "$LOG_FILE" | tail -1 | grep -oP 'now has \K\d+' || echo "0")
NODE3=$(grep "RECORDED: Node 3" "$LOG_FILE" | tail -1 | grep -oP 'now has \K\d+' || echo "0")

TOTAL=$((NODE1 + NODE2 + NODE3))
AVG=$((TOTAL / 3))
echo "Statistics:"
echo "  Total requests processed: $TOTAL"
echo "  Average per node: $AVG"
echo "  Expected per node: $((TOTAL_REQUESTS / 3))"

# Check if distribution is fair (within 20% variance)
MAX_VARIANCE=$((AVG / 5))  # 20% of average

for node in "$NODE1" "$NODE2" "$NODE3"; do
    diff=$((node > AVG ? node - AVG : AVG - node))
    if [ $diff -gt $MAX_VARIANCE ]; then
        echo ""
        echo "WARNING: Load imbalance detected! Variance: $diff (max allowed: $MAX_VARIANCE)"
        exit 1
    fi
done

echo ""
echo "✅ PASS: Load distribution is balanced"
echo ""
echo "Log file: $LOG_FILE"
echo "Test image: $TEST_IMAGE"
echo ""
echo "To see detailed logs: cat $LOG_FILE"
echo "To verify images: ls -lh shared_images/"