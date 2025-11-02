#!/bin/bash

# Multi-Machine Stress Test for Distributed Load Balancer
# Tests load distribution across different physical machines

set -e

echo "========================================"
echo "  MULTI-MACHINE LOAD BALANCER TEST"
echo "========================================"
echo ""

# Configuration
NUM_CLIENTS=10
REQUESTS_PER_CLIENT=10
TOTAL_REQUESTS=$((NUM_CLIENTS * REQUESTS_PER_CLIENT))
TEST_IMAGE="test_image.bin"
LOG_FILE="stress_test_multi_$(date +%s).log"

# Read machine IPs from config.json
if [ ! -f "config.json" ]; then
    echo "ERROR: config.json not found!"
    echo "Please create config.json with your machine IPs."
    exit 1
fi

echo "Machine Configuration (from config.json):"
echo "----------------------------------------"
cat config.json | grep "address" | while read line; do
    echo "  $line"
done
echo ""

# Create a test image if it doesn't exist
if [ ! -f "$TEST_IMAGE" ]; then
    echo "Creating test image ($TEST_IMAGE)..."
    dd if=/dev/urandom of="$TEST_IMAGE" bs=1024 count=100 2>/dev/null
    echo "✓ Test image created (100KB)"
fi

echo ""
echo "Test Configuration:"
echo "  - Concurrent Clients: $NUM_CLIENTS"
echo "  - Requests per Client: $REQUESTS_PER_CLIENT"
echo "  - Total Requests: $TOTAL_REQUESTS"
echo "  - Test Image: $TEST_IMAGE"
echo "  - Log File: $LOG_FILE"
echo ""

# Verify connectivity to all nodes
echo "Checking connectivity to server nodes..."
echo "----------------------------------------"

# Extract IPs and ports from config.json
NODE1_ADDR=$(grep -A1 '"id": 1' config.json | grep address | cut -d'"' -f4)
NODE2_ADDR=$(grep -A1 '"id": 2' config.json | grep address | cut -d'"' -f4)
NODE3_ADDR=$(grep -A1 '"id": 3' config.json | grep address | cut -d'"' -f4)

NODE1_IP=$(echo $NODE1_ADDR | cut -d':' -f1)
NODE1_PORT=$(echo $NODE1_ADDR | cut -d':' -f2)
NODE2_IP=$(echo $NODE2_ADDR | cut -d':' -f1)
NODE2_PORT=$(echo $NODE2_ADDR | cut -d':' -f2)
NODE3_IP=$(echo $NODE3_ADDR | cut -d':' -f1)
NODE3_PORT=$(echo $NODE3_ADDR | cut -d':' -f2)

# Test connectivity
test_connection() {
    local ip=$1
    local port=$2
    local node=$3

    if timeout 2 bash -c "echo > /dev/tcp/$ip/$port" 2>/dev/null; then
        echo "✓ Node $node ($ip:$port) - REACHABLE"
        return 0
    else
        echo "✗ Node $node ($ip:$port) - UNREACHABLE"
        return 1
    fi
}

CONNECTIVITY_OK=true
test_connection $NODE1_IP $NODE1_PORT 1 || CONNECTIVITY_OK=false
test_connection $NODE2_IP $NODE2_PORT 2 || CONNECTIVITY_OK=false
test_connection $NODE3_IP $NODE3_PORT 3 || CONNECTIVITY_OK=false

echo ""

if [ "$CONNECTIVITY_OK" = false ]; then
    echo "ERROR: Cannot reach all server nodes!"
    echo ""
    echo "Please ensure:"
    echo "1. All 3 server nodes are running"
    echo "2. Firewall allows ports 7001-7003"
    echo "3. config.json has correct IP addresses"
    echo ""
    echo "Start servers with:"
    echo "  Machine 1: SHARED_DIR=./shared_images NODE_ID=1 cargo run --release --bin cloud-p2p"
    echo "  Machine 2: SHARED_DIR=./shared_images NODE_ID=2 cargo run --release --bin cloud-p2p"
    echo "  Machine 3: SHARED_DIR=./shared_images NODE_ID=3 cargo run --release --bin cloud-p2p"
    exit 1
fi

echo "All nodes reachable. Waiting 3 seconds for leader election..."
sleep 3

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
                grep -E "(LEADER|WORKER|RECORDED|SELECTED|Forwarding)" || true
        done
    ) >> "$LOG_FILE" 2>&1 &

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
if [ $DURATION -gt 0 ]; then
    echo "Throughput: $((TOTAL_REQUESTS / DURATION)) requests/second"
else
    echo "Throughput: Very fast (< 1 second)"
fi
echo ""

# Analyze results
echo "========================================"
echo "  LOAD DISTRIBUTION ANALYSIS"
echo "========================================"
echo ""

echo "Final load counts from leader's balancer:"
grep "RECORDED: Node" "$LOG_FILE" | tail -3 || echo "No load records found in log"

echo ""
echo "Distribution across physical machines:"

# Extract final counts for each node
NODE1=$(grep "RECORDED: Node 1" "$LOG_FILE" | tail -1 | grep -oP 'now has \K\d+' || echo "0")
NODE2=$(grep "RECORDED: Node 2" "$LOG_FILE" | tail -1 | grep -oP 'now has \K\d+' || echo "0")
NODE3=$(grep "RECORDED: Node 3" "$LOG_FILE" | tail -1 | grep -oP 'now has \K\d+' || echo "0")

echo "  Machine $NODE1_ADDR (Node 1): $NODE1 requests"
echo "  Machine $NODE2_ADDR (Node 2): $NODE2 requests"
echo "  Machine $NODE3_ADDR (Node 3): $NODE3 requests"

TOTAL=$((NODE1 + NODE2 + NODE3))
AVG=$((TOTAL / 3))

echo ""
echo "Statistics:"
echo "  Total requests processed: $TOTAL"
echo "  Expected total: $TOTAL_REQUESTS"
echo "  Average per machine: $AVG"
echo "  Expected per machine: $((TOTAL_REQUESTS / 3))"

# Calculate variance and check fairness
MAX_VARIANCE=$((AVG / 5))  # 20% tolerance

echo ""
echo "Variance check (max allowed: $MAX_VARIANCE):"

FAILED=false
for node_count in $NODE1 $NODE2 $NODE3; do
    diff=$((node_count > AVG ? node_count - AVG : AVG - node_count))
    if [ $diff -gt $MAX_VARIANCE ]; then
        echo "  ✗ Variance $diff exceeds threshold!"
        FAILED=true
    else
        echo "  ✓ Variance $diff within tolerance"
    fi
done

echo ""

if [ "$FAILED" = true ]; then
    echo "❌ FAIL: Load distribution is UNBALANCED across machines"
    echo ""
    echo "Possible causes:"
    echo "- One node might be down or slow"
    echo "- Network latency differences"
    echo "- Load balancer bug"
    exit 1
fi

# Check if total matches expected
if [ $TOTAL -lt $((TOTAL_REQUESTS - 5)) ]; then
    echo "⚠️  WARNING: Some requests may have been lost"
    echo "  Expected: $TOTAL_REQUESTS"
    echo "  Processed: $TOTAL"
    echo "  Missing: $((TOTAL_REQUESTS - TOTAL))"
else
    echo "✅ PASS: Load distribution is balanced across physical machines"
fi

echo ""
echo "Network Distribution Summary:"
echo "  Node 1 ($NODE1_IP): $(awk "BEGIN {printf \"%.1f\", $NODE1*100/$TOTAL}")%"
echo "  Node 2 ($NODE2_IP): $(awk "BEGIN {printf \"%.1f\", $NODE2*100/$TOTAL}")%"
echo "  Node 3 ($NODE3_IP): $(awk "BEGIN {printf \"%.1f\", $NODE3*100/$TOTAL}")%"

echo ""
echo "========================================"
echo "  VERIFICATION STEPS"
echo "========================================"
echo ""
echo "To verify files were saved on each machine:"
echo ""
echo "  On $NODE1_IP (Node 1):"
echo "    ssh user@$NODE1_IP 'ls -lh shared_images/ | wc -l'"
echo "    # Should show approximately $NODE1 files"
echo ""
echo "  On $NODE2_IP (Node 2):"
echo "    ssh user@$NODE2_IP 'ls -lh shared_images/ | wc -l'"
echo "    # Should show approximately $NODE2 files"
echo ""
echo "  On $NODE3_IP (Node 3):"
echo "    ssh user@$NODE3_IP 'ls -lh shared_images/ | wc -l'"
echo "    # Should show approximately $NODE3 files"
echo ""
echo "Detailed log: $LOG_FILE"
echo "Test image: $TEST_IMAGE"
echo ""