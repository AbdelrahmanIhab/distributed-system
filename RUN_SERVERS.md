# How to Run the Distributed System

This guide shows you how to run 3 servers on 3 different machines and then run the stress test client.

---

## Step 1: Prepare All 3 Machines

On **each of the 3 server machines**, clone the repo and copy the config.json:

```bash
git clone <your-repo-url> distributed-system
cd distributed-system
```

Make sure **all 3 machines have the same config.json**:

```json
{
  "peers": [
    {
      "id": 1,
      "address": "10.40.45.206:7001"
    },
    {
      "id": 2,
      "address": "10.40.41.192:7002"
    },
    {
      "id": 3,
      "address": "10.40.53.40:7003"
    }
  ],
  "shared_dir": "./shared_images"
}
```

---

## Step 2: Build the Project (on each machine)

On each machine:

```bash
cd ~/distributed-system
cargo build --release
```

This will take a few minutes the first time.

---

## Step 3: Start the Server Nodes

### On Machine 1 (IP: 10.40.45.206)

```bash
cd ~/distributed-system
mkdir -p shared_images
CONFIG_FILE=./config.json SHARED_DIR=./shared_images NODE_ID=1 cargo run --release --bin cloud-p2p
```

You should see:
```
[Config] Loading from file: ./config.json
[Node 1] Shared directory: ./shared_images
[Node 1] Starting on 10.40.45.206:7001
[Node 1] Peer configuration: ...
```

---

### On Machine 2 (IP: 10.40.41.192)

```bash
cd ~/distributed-system
mkdir -p shared_images
CONFIG_FILE=./config.json SHARED_DIR=./shared_images NODE_ID=2 cargo run --release --bin cloud-p2p
```

You should see:
```
[Config] Loading from file: ./config.json
[Node 2] Shared directory: ./shared_images
[Node 2] Starting on 10.40.41.192:7002
[Node 2] Peer configuration: ...
```

---

### On Machine 3 (IP: 10.40.53.40)

```bash
cd ~/distributed-system
mkdir -p shared_images
CONFIG_FILE=./config.json SHARED_DIR=./shared_images NODE_ID=3 cargo run --release --bin cloud-p2p
```

You should see:
```
[Config] Loading from file: ./config.json
[Node 3] Shared directory: ./shared_images
[Node 3] Starting on 10.40.53.40:7003
[Node 3] Peer configuration: ...
```

---

## Step 4: Wait for Leader Election

After starting all 3 nodes, wait about **5-10 seconds**. You should see one of them (likely Node 3) print:

```
[Node 3] no higher nodes -> becoming coordinator
[Node 3] Broadcasting Coordinator message
```

This means Node 3 is now the **leader**.

---

## Step 5: Run the Stress Test Client

From **any machine** that can reach all 3 servers (can be one of the server machines or a 4th machine):

```bash
cd ~/distributed-system
./stress_test_1000_images.sh
```

The script will:
1. ✅ Generate 1000 test images
2. ✅ Spawn 50 concurrent clients
3. ✅ Send all images to the leader
4. ✅ Leader distributes work across all 3 nodes
5. ✅ Show results and throughput

---

## Step 6: Verify Load Distribution

After the test completes, check how many images each server received:

**On Machine 1:**
```bash
ls -1 ~/distributed-system/shared_images | wc -l
```

**On Machine 2:**
```bash
ls -1 ~/distributed-system/shared_images | wc -l
```

**On Machine 3:**
```bash
ls -1 ~/distributed-system/shared_images | wc -l
```

Each should have roughly **~333 images** if load balancing is working correctly.

---

## Troubleshooting

### Problem: "Connection refused"

**Solution:** Make sure:
- All 3 server nodes are running
- Firewall allows ports 7001, 7002, 7003
- The IP addresses in config.json are correct

Test connectivity from the client machine:
```bash
nc -zv 10.40.45.206 7001
nc -zv 10.40.41.192 7002
nc -zv 10.40.53.40 7003
```

All should say "succeeded" or "open".

---

### Problem: "No config file or env found, using default localhost"

**Solution:** Make sure you set `CONFIG_FILE=./config.json` when starting the servers:

```bash
CONFIG_FILE=./config.json NODE_ID=1 cargo run --release --bin cloud-p2p
```

---

### Problem: Server binds to 127.0.0.1 instead of its real IP

**Solution:** Check that config.json has the correct IP addresses, not localhost addresses.

---

## Clean Up

After testing:

**On each server machine:**
```bash
# Stop the server (Ctrl+C)
# Remove received images
rm -rf ~/distributed-system/shared_images/*
```

**On the client machine:**
```bash
# Remove test data
rm -rf ~/distributed-system/test_images_stress
rm -f ~/distributed-system/stress_test_client_*.log
```

---

## Quick Reference Commands

### Start Server Node 1:
```bash
cd ~/distributed-system && CONFIG_FILE=./config.json SHARED_DIR=./shared_images NODE_ID=1 cargo run --release --bin cloud-p2p
```

### Start Server Node 2:
```bash
cd ~/distributed-system && CONFIG_FILE=./config.json SHARED_DIR=./shared_images NODE_ID=2 cargo run --release --bin cloud-p2p
```

### Start Server Node 3:
```bash
cd ~/distributed-system && CONFIG_FILE=./config.json SHARED_DIR=./shared_images NODE_ID=3 cargo run --release --bin cloud-p2p
```

### Run Stress Test:
```bash
cd ~/distributed-system && ./stress_test_1000_images.sh
```