#!/bin/bash

# Multi-Machine Stress Test: 1000 images across 3 remote nodes
# This script runs on a client machine and sends images to 3 separate server nodes
# The servers are running on different physical machines

set -e

# Configuration
TOTAL_IMAGES=1000
CONCURRENT_CLIENTS=50  # Number of parallel client processes
IMAGES_PER_CLIENT=$((TOTAL_IMAGES / CONCURRENT_CLIENTS))
TEST_IMAGE_SIZE=512000  # 500KB per image
CONFIG_FILE="./config.json"

# Client will bind to different ports for each concurrent instance
CLIENT_BASE_PORT=7100

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo "========================================"
echo "  STRESS TEST: 1000 IMAGES"
echo "========================================"
echo ""

# Check if config.json exists
if [ ! -f "$CONFIG_FILE" ]; then
    echo -e "${RED}Error: config.json not found${NC}"
    exit 1
fi

# Parse config.json to get node addresses
echo -e "${BLUE}Configured Nodes:${NC}"
cat $CONFIG_FILE | grep -A 5 "peers" | grep "address" | awk -F'"' '{print "  - " $4}'
echo ""

# Check if project is built
if [ ! -f "target/release/client" ]; then
    echo -e "${YELLOW}Building project...${NC}"
    cargo build --release
    echo -e "${GREEN}Build complete${NC}"
    echo ""
fi

# Create test images directory
TEST_IMAGE_DIR="./test_images_stress"
rm -rf $TEST_IMAGE_DIR
mkdir -p $TEST_IMAGE_DIR

echo -e "${BLUE}Test Configuration:${NC}"
echo "  - Total images: $TOTAL_IMAGES"
echo "  - Concurrent clients: $CONCURRENT_CLIENTS"
echo "  - Images per client: $IMAGES_PER_CLIENT"
echo "  - Image size: $(($TEST_IMAGE_SIZE / 1024))KB"
echo ""

# Generate test images
echo -e "${YELLOW}Generating $TOTAL_IMAGES test images...${NC}"
for i in $(seq 1 $TOTAL_IMAGES); do
    dd if=/dev/urandom of="$TEST_IMAGE_DIR/image_$i.bin" bs=$TEST_IMAGE_SIZE count=1 2>/dev/null
    if [ $((i % 100)) -eq 0 ]; then
        echo "  Generated $i/$TOTAL_IMAGES images..."
    fi
done
echo -e "${GREEN}✓ Generated $TOTAL_IMAGES test images${NC}"
echo ""

# Function to send images from a single client
send_images() {
    local client_id=$1
    local start_idx=$2
    local end_idx=$3
    local log_file="stress_test_client_${client_id}.log"

    > $log_file  # Clear log file

    # Build a command script for this client
    local cmd_script=$(mktemp)

    # First discover the leader
    echo "discover" >> $cmd_script

    # Add send-image commands for all images assigned to this client
    for i in $(seq $start_idx $end_idx); do
        echo "send-image $TEST_IMAGE_DIR/image_$i.bin" >> $cmd_script
    done

    # Feed all commands to a single client instance
    CONFIG_FILE="$CONFIG_FILE" timeout 120 target/release/client < $cmd_script >> $log_file 2>&1 || true

    # Clean up temp file
    rm -f $cmd_script

    # Count successes
    local success_count=$(grep -c "SUCCESS" $log_file || echo "0")
    echo "$client_id:$success_count"
}

export -f send_images
export TEST_IMAGE_DIR

# Clear previous logs
rm -f stress_test_client_*.log

echo -e "${YELLOW}Starting stress test with $CONCURRENT_CLIENTS concurrent clients...${NC}"
echo ""

START_TIME=$(date +%s)

# Launch concurrent clients
pids=()
for client_id in $(seq 1 $CONCURRENT_CLIENTS); do
    start_idx=$(( (client_id - 1) * IMAGES_PER_CLIENT + 1 ))
    end_idx=$(( client_id * IMAGES_PER_CLIENT ))

    # Handle remaining images for the last client
    if [ $client_id -eq $CONCURRENT_CLIENTS ]; then
        end_idx=$TOTAL_IMAGES
    fi

    echo -e "${BLUE}[Client $client_id]${NC} Sending images $start_idx to $end_idx"

    # Launch client in background
    send_images $client_id $start_idx $end_idx &
    pids+=($!)
done

echo ""
echo -e "${YELLOW}Waiting for all clients to complete...${NC}"

# Wait for all clients to finish
for pid in "${pids[@]}"; do
    wait $pid
done

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

echo ""
echo -e "${GREEN}✓ All clients completed${NC}"
echo ""

# Analyze results
echo "========================================"
echo "  RESULTS ANALYSIS"
echo "========================================"
echo ""

# Count total successes and failures
TOTAL_SUCCESS=0
TOTAL_FAILED=0

for client_id in $(seq 1 $CONCURRENT_CLIENTS); do
    log_file="stress_test_client_${client_id}.log"
    if [ -f "$log_file" ]; then
        success=$(grep -c "SUCCESS" $log_file || echo "0")
        failed=$(grep -c "FAILED" $log_file || echo "0")
        TOTAL_SUCCESS=$((TOTAL_SUCCESS + success))
        TOTAL_FAILED=$((TOTAL_FAILED + failed))
    fi
done

echo -e "${BLUE}Performance Metrics:${NC}"
echo "  - Total images sent: $TOTAL_IMAGES"
echo "  - Successfully processed: $TOTAL_SUCCESS"
echo "  - Failed: $TOTAL_FAILED"
echo "  - Duration: ${DURATION}s"
echo "  - Throughput: $(echo "scale=2; $TOTAL_SUCCESS / $DURATION" | bc) images/sec"
echo ""

# Check distribution across nodes (from server logs or shared_images directory)
echo -e "${BLUE}Load Distribution Across Nodes:${NC}"

# Count files in shared_images directory if it exists
if [ -d "./shared_images" ]; then
    local_count=$(ls -1 ./shared_images 2>/dev/null | wc -l)
    echo "  - Local node (this machine): $local_count images"
else
    echo "  - Local node: No shared_images directory"
fi

echo ""
echo -e "${YELLOW}To check load distribution across all 3 remote nodes:${NC}"

# Extract node IPs from config.json
NODE1_IP=$(grep -A 10 '"id": 1' $CONFIG_FILE | grep "address" | awk -F'"' '{print $4}' | cut -d':' -f1)
NODE2_IP=$(grep -A 10 '"id": 2' $CONFIG_FILE | grep "address" | awk -F'"' '{print $4}' | cut -d':' -f1)
NODE3_IP=$(grep -A 10 '"id": 3' $CONFIG_FILE | grep "address" | awk -F'"' '{print $4}' | cut -d':' -f1)

echo ""
echo "Run these commands to check each node:"
echo -e "${BLUE}  ssh user@$NODE1_IP 'ls -1 ~/distributed-system/shared_images | wc -l'${NC}"
echo -e "${BLUE}  ssh user@$NODE2_IP 'ls -1 ~/distributed-system/shared_images | wc -l'${NC}"
echo -e "${BLUE}  ssh user@$NODE3_IP 'ls -1 ~/distributed-system/shared_images | wc -l'${NC}"
echo ""
echo "The load should be roughly evenly distributed (~333 images per node for 1000 total)"
echo ""

# Success criteria
if [ $TOTAL_SUCCESS -ge $((TOTAL_IMAGES * 95 / 100)) ]; then
    echo -e "${GREEN}✅ PASS: Stress test completed successfully (${TOTAL_SUCCESS}/${TOTAL_IMAGES} images processed)${NC}"
else
    echo -e "${RED}❌ FAIL: Too many failures (${TOTAL_FAILED}/${TOTAL_IMAGES} failed)${NC}"
fi

echo ""
echo "========================================"
echo "  CLEANUP"
echo "========================================"
echo ""
echo "To clean up test data:"
echo "  rm -rf $TEST_IMAGE_DIR"
echo "  rm -f stress_test_client_*.log"
echo ""
echo "To clean up shared images on all nodes:"
echo "  rm -rf ./shared_images/*"
echo ""