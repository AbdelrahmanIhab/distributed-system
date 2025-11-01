#!/bin/bash

# Quick local test script - runs 3 nodes on localhost in separate terminals

echo "Starting 3-node distributed system on localhost..."
echo ""
echo "This script will open 3 terminal windows, each running a node."
echo "Make sure you have a terminal emulator installed (gnome-terminal, xterm, etc.)"
echo ""

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
    echo "No supported terminal emulator found. Please run nodes manually:"
    echo ""
    echo "Terminal 1: NODE_ID=1 cargo run --release"
    echo "Terminal 2: NODE_ID=2 cargo run --release"
    echo "Terminal 3: NODE_ID=3 cargo run --release"
    exit 1
fi

echo "Starting nodes in separate terminals..."

# Start node 1
$TERM_CMD bash -c "NODE_ID=1 cargo run --release; read -p 'Press Enter to close...'" &
sleep 1

# Start node 2
$TERM_CMD bash -c "NODE_ID=2 cargo run --release; read -p 'Press Enter to close...'" &
sleep 1

# Start node 3
$TERM_CMD bash -c "NODE_ID=3 cargo run --release; read -p 'Press Enter to close...'" &

echo ""
echo "All nodes started!"
echo "Watch the terminal windows for election and heartbeat messages."
echo ""
echo "To stop: close each terminal window or press Ctrl+C in each"
