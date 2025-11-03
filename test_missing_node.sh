#!/bin/bash

# Test script to verify improved Bully algorithm with missing Node 2

set -e

echo "=========================================="
echo "  Testing Bully Algorithm with Missing Node"
echo "=========================================="
echo ""

# Kill any existing processes
pkill -9 -f "cloud-p2p" 2>/dev/null || true
sleep 1

# Clean up
rm -rf /tmp/node*.log

echo "Starting Node 1..."
export NODE_ID=1
export CONFIG_FILE=./config_localhost.json
./target/release/cloud-p2p > /tmp/node1.log 2>&1 &
NODE1_PID=$!
echo "✓ Node 1 started (PID: $NODE1_PID)"

echo ""
echo "Starting Node 3 (Node 2 is MISSING)..."
export NODE_ID=3
export CONFIG_FILE=./config_localhost.json
./target/release/cloud-p2p > /tmp/node3.log 2>&1 &
NODE3_PID=$!
echo "✓ Node 3 started (PID: $NODE3_PID)"

echo ""
echo "⚠️  NOTE: Node 2 is NOT running (simulating node failure)"
echo ""

# Cleanup function
cleanup() {
    echo ""
    echo "Stopping all nodes..."
    kill $NODE1_PID $NODE3_PID 2>/dev/null || true
    echo "✓ All nodes stopped"
}

trap cleanup EXIT

echo "Waiting 15 seconds to observe election behavior..."
echo "(Watch for election cooldown messages and stabilization)"
echo ""

sleep 15

echo "=========================================="
echo "  Election Results After 15 Seconds"
echo "=========================================="
echo ""

echo "Node 1 Election Activity:"
grep -E "ELECTION|LEADER|unreachable|Cooldown" /tmp/node1.log | tail -20
echo ""

echo "Node 3 Election Activity:"
grep -E "ELECTION|LEADER|unreachable|Cooldown" /tmp/node3.log | tail -20
echo ""

echo "=========================================="
echo "  Analysis"
echo "=========================================="
echo ""

# Count elections
NODE1_ELECTIONS=$(grep -c "Starting election process" /tmp/node1.log || echo "0")
NODE3_ELECTIONS=$(grep -c "Starting election process" /tmp/node3.log || echo "0")

echo "Node 1 started $NODE1_ELECTIONS election(s)"
echo "Node 3 started $NODE3_ELECTIONS election(s)"
echo ""

# Check if Node 3 became leader
if grep -q "Becoming LEADER" /tmp/node3.log; then
    echo "✅ Node 3 successfully became LEADER (highest priority)"
else
    echo "✗ Node 3 did not become leader"
fi

# Check if cooldown is working
COOLDOWN_COUNT=$(grep -c "Cooldown active" /tmp/node*.log || echo "0")
if [ "$COOLDOWN_COUNT" -gt 0 ]; then
    echo "✅ Election cooldown is working ($COOLDOWN_COUNT cooldown messages)"
else
    echo "✗ No cooldown messages detected"
fi

# Check for unreachable node detection
UNREACHABLE_COUNT=$(grep -c "unreachable" /tmp/node*.log || echo "0")
if [ "$UNREACHABLE_COUNT" -gt 0 ]; then
    echo "✅ Unreachable nodes detected ($UNREACHABLE_COUNT detections)"
else
    echo "ℹ️  No unreachable node detections"
fi

echo ""
echo "=========================================="
echo "  Test Complete"
echo "=========================================="
echo ""
echo "The system should have:"
echo "  1. Detected Node 2 as unreachable"
echo "  2. Node 3 became leader (highest reachable node)"
echo "  3. Election cooldown prevented rapid re-elections"
echo "  4. System stabilized with 2 nodes"
