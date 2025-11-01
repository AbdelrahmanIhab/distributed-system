#!/bin/bash

# Quick local test script - runs 3 server nodes + 1 client on localhost in separate terminals

echo "Starting 3-node distributed system + client on localhost..."
echo ""
echo "This script will open 4 terminal windows: 3 servers + 1 client."
echo "Make sure you have a terminal emulator installed (gnome-terminal, xterm, etc.)"
echo ""

# Create shared directory for images
SHARED_DIR="./shared_images"
echo "Creating shared directory: $SHARED_DIR"
mkdir -p "$SHARED_DIR"

# Build first
echo "Building project..."
cargo build --release

# Detect terminal emulator
if command -v gnome-terminal &> /dev/null; then
    TERM_CMD="gnome-terminal --"
elif command -v xterm &> /dev/null; then
    TERM_CMD="xterm -e"
elif command -v konsole &> /dev/null; then
    TERM_CMD="konsole -e"
else
    echo "No supported terminal emulator found. Please run manually:"
    echo ""
    echo "Terminal 1: SHARED_DIR=$SHARED_DIR NODE_ID=1 cargo run --release --bin cloud-p2p"
    echo "Terminal 2: SHARED_DIR=$SHARED_DIR NODE_ID=2 cargo run --release --bin cloud-p2p"
    echo "Terminal 3: SHARED_DIR=$SHARED_DIR NODE_ID=3 cargo run --release --bin cloud-p2p"
    echo "Terminal 4: cargo run --release --bin client"
    exit 1
fi

echo "Starting servers in separate terminals..."

# Start server node 1
$TERM_CMD bash -c "SHARED_DIR=$SHARED_DIR NODE_ID=1 cargo run --release --bin cloud-p2p; read -p 'Press Enter to close...'" &
sleep 1

# Start server node 2
$TERM_CMD bash -c "SHARED_DIR=$SHARED_DIR NODE_ID=2 cargo run --release --bin cloud-p2p; read -p 'Press Enter to close...'" &
sleep 1

# Start server node 3
$TERM_CMD bash -c "SHARED_DIR=$SHARED_DIR NODE_ID=3 cargo run --release --bin cloud-p2p; read -p 'Press Enter to close...'" &
sleep 2

# Start client
$TERM_CMD bash -c "cargo run --release --bin client; read -p 'Press Enter to close...'" &

echo ""
echo "All nodes started!"
echo "  - 3 server nodes running with leader election"
echo "  - 1 client ready to send images"
echo "  - Shared directory: $SHARED_DIR"
echo ""
echo "In the client terminal, use: send-image <path/to/image.jpg>"
echo "Images will be stored in: $SHARED_DIR"
echo ""
echo "To stop: close each terminal window or press Ctrl+C in each"
