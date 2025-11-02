# Multi-Machine Stress Test Guide

This guide explains how to run the stress test across multiple physical machines to verify true distributed load balancing.

## Prerequisites

- **3 machines** for server nodes (can use VMs or physical machines)
- **1 machine** for running the client stress test (can be one of the server machines)
- All machines must be on the **same network**
- Machines must be able to reach each other on ports **7001, 7002, 7003**

---

## Step 1: Get Your Machine IP Addresses

On each machine, find its IP address:

```bash
# Linux/Mac
ip addr show | grep "inet " | grep -v 127.0.0.1

# Or simpler
hostname -I
```

**Example IPs:**
- Machine 1: `192.168.1.100`
- Machine 2: `192.168.1.101`
- Machine 3: `192.168.1.102`

---

## Step 2: Update `config.json` on ALL Machines

Create/edit `config.json` on **each machine** with the real IP addresses:

```json
{
  "peers": [
    {"id": 1, "address": "192.168.1.100:7001"},
    {"id": 2, "address": "192.168.1.101:7002"},
    {"id": 3, "address": "192.168.1.102:7003"}
  ],
  "shared_dir": "./shared_images"
}
```

⚠️ **IMPORTANT:** All 3 machines must have **identical** `config.json` files!

---

## Step 3: Copy the Project to Each Machine

On **each machine**, you need the compiled binaries:

### Option A: Build on each machine
```bash
# On each machine
git clone <your-repo> distributed-system
cd distributed-system
cargo build --release
```

### Option B: Copy binaries from one machine
```bash
# On Machine 1 (after building)
scp target/release/cloud-p2p user@192.168.1.101:~/
scp target/release/cloud-p2p user@192.168.1.102:~/
scp target/release/client user@192.168.1.101:~/
scp config.json user@192.168.1.101:~/
scp config.json user@192.168.1.102:~/
```

---

## Step 4: Start Server Nodes

### On Machine 1 (192.168.1.100)
```bash
cd distributed-system
SHARED_DIR=./shared_images NODE_ID=1 cargo run --release --bin cloud-p2p
```

### On Machine 2 (192.168.1.101)
```bash
cd distributed-system
SHARED_DIR=./shared_images NODE_ID=2 cargo run --release --bin cloud-p2p
```

### On Machine 3 (192.168.1.102)
```bash
cd distributed-system
SHARED_DIR=./shared_images NODE_ID=3 cargo run --release --bin cloud-p2p
```

**Wait ~5 seconds** for leader election to complete. You should see:
```
[Node 3] no higher nodes -> becoming coordinator
```

---

## Step 5: Verify Connectivity

From the client machine, test if you can reach all servers:

```bash
# Test connectivity to all nodes
nc -zv 192.168.1.100 7001
nc -zv 192.168.1.101 7002
nc -zv 192.168.1.102 7003
```

All should respond with "succeeded" or "open".

---

## Step 6: Update Client Configuration

On the **client machine** (can be any of the 3 or a 4th machine), update `config.json`:

```json
{
  "peers": [
    {"id": 1, "address": "192.168.1.100:7001"},
    {"id": 2, "address": "192.168.1.101:7002"},
    {"id": 3, "address": "192.168.1.102:7003"}
  ],
  "shared_dir": "./shared_images"
}
```

---

## Step 7: Run the Multi-Machine Stress Test

Copy the stress test script to your client machine, then run:

```bash
./stress_test_multi_machine.sh
```

This will:
1. Create a test image
2. Launch 10 concurrent clients
3. Each client sends 10 requests (100 total)
4. Measure load distribution across the 3 **physical machines**
5. Verify balanced distribution

---

## Expected Output

```
========================================
  MULTI-MACHINE LOAD BALANCER STRESS TEST
========================================

Machine IPs:
  Node 1: 192.168.1.100:7001
  Node 2: 192.168.1.101:7002
  Node 3: 192.168.1.102:7003

Test Configuration:
  - Concurrent Clients: 10
  - Requests per Client: 10
  - Total Requests: 100

...

========================================
  LOAD DISTRIBUTION ANALYSIS
========================================

Distribution across physical machines:
  Machine 192.168.1.100 (Node 1): 33 requests
  Machine 192.168.1.101 (Node 2): 34 requests
  Machine 192.168.1.102 (Node 3): 33 requests

✅ PASS: Load distribution is balanced across machines
```

---

## Verification Steps

### 1. Check files on each machine

**On Machine 1:**
```bash
ls -lh shared_images/ | wc -l
# Should show ~33 files
```

**On Machine 2:**
```bash
ls -lh shared_images/ | wc -l
# Should show ~34 files
```

**On Machine 3:**
```bash
ls -lh shared_images/ | wc -l
# Should show ~33 files
```

### 2. Check server logs

Each server should show:
```
[LeastLoad] RECORDED: Node X now has Y total requests
[Node X] ✓ Saved image to "./shared_images/..."
```

### 3. Verify network traffic

On each server machine:
```bash
# Monitor network activity during test
sudo tcpdump -i any port 7001 or port 7002 or port 7003
```

You should see TCP traffic between all nodes.

---

## Troubleshooting

### Problem: "Connection refused"
**Solution:**
- Check firewall rules: `sudo ufw allow 7001:7003/tcp`
- Verify servers are running: `ps aux | grep cloud-p2p`
- Check IP addresses in config.json are correct

### Problem: "No leader elected"
**Solution:**
- Wait 10 seconds after starting all servers
- Check all servers have the same config.json
- Restart all servers in order: Node 1, then 2, then 3

### Problem: "Uneven distribution"
**Solution:**
- Check all servers are actually running (not crashed)
- Verify client is connecting to the leader (highest Node ID)
- Check server logs for errors

---

## Clean Up

After testing, you can:

1. **Stop all servers:** Press `Ctrl+C` on each machine
2. **Remove test data:**
   ```bash
   # On each machine
   rm -rf shared_images/
   rm test_image.bin
   ```
3. **Remove logs:**
   ```bash
   rm stress_test_*.log
   ```

---

## Performance Metrics to Collect

When running the multi-machine test, collect:

1. **Throughput:** Requests per second across all machines
2. **Latency:** Average time per request (includes network delay)
3. **Network usage:** Bandwidth consumed between machines
4. **Load distribution variance:** Standard deviation of requests per node

Example results to document:
```
Total requests: 100
Duration: 15 seconds
Throughput: 6.67 req/sec
Average latency: 150ms
Network bandwidth: ~10KB/s per node
Distribution: Node1=33, Node2=34, Node3=33 (variance: ±0.47)
```

This proves the system works across **real network boundaries** with **true distributed load balancing**!