# Manual Multi-Machine Setup Guide

Simple step-by-step instructions to deploy the distributed system across 3 computers.

---

## Prerequisites

- 3 computers on the same network
- Know the IP address of each computer
- Rust installed on all machines

---

## Step 1: Find IP Addresses

On each computer, run:

**Linux:**
```bash
ip addr show
# or
hostname -I
```

**macOS:**
```bash
ifconfig | grep "inet " | grep -v 127.0.0.1
# or
ipconfig getifaddr en0  # for WiFi
ipconfig getifaddr en1  # for Ethernet
```

Example IPs used in this guide:
- **Machine 1** (Linux): `192.168.1.100`
- **Machine 2** (Linux): `192.168.1.101`
- **Machine 3** (macOS): `192.168.1.102`

---

## Step 2: Copy Project to All Machines

On your development machine:
```bash
# Option 1: Using rsync
rsync -avz --exclude target . user@192.168.1.100:~/distributed-system/
rsync -avz --exclude target . user@192.168.1.101:~/distributed-system/
rsync -avz --exclude target . user@192.168.1.102:~/distributed-system/

# Option 2: Using git (recommended)
# On each machine:
git clone <your-repo-url>
cd distributed-system
```

---

## Step 3: Create config.json on Each Machine

On **all 3 machines**, create `~/distributed-system/config.json` with **your actual IP addresses**:

```json
{
  "peers": [
    {
      "id": 1,
      "address": "192.168.1.100:7001"
    },
    {
      "id": 2,
      "address": "192.168.1.101:7002"
    },
    {
      "id": 3,
      "address": "192.168.1.102:7003"
    }
  ],
  "shared_dir": "/home/youruser/images"
}
```

**Important:** Replace:
- IP addresses with your actual IPs
- `/home/youruser/images` with your actual username

---

## Step 4: Setup on Each Machine

Run these commands **on each of the 3 server machines**:

```bash
cd ~/distributed-system

# Create images directory
mkdir -p ~/images

# Build the project
cargo build --release
```

### Firewall Setup (change port for each machine)

**On Linux (ufw):**
```bash
# Machine 1:
sudo ufw allow 7001/tcp

# Machine 2:
sudo ufw allow 7002/tcp

# Machine 3:
sudo ufw allow 7003/tcp
```

**On macOS:**
```bash
# macOS firewall usually allows outbound connections by default
# If you have firewall enabled and face issues:
# System Settings > Network > Firewall > Firewall Options
# Add "cloud-p2p" and allow incoming connections

# Or temporarily disable firewall for testing (not recommended):
# sudo /usr/libexec/ApplicationFirewall/socketfilterfw --setglobalstate off
```

---

## Step 5: Start Servers

Run on **each machine** (change NODE_ID):

**Machine 1 (192.168.1.100):**
```bash
cd ~/distributed-system
CONFIG_FILE=config.json NODE_ID=1 ./target/release/cloud-p2p
```

**Machine 2 (192.168.1.101):**
```bash
cd ~/distributed-system
CONFIG_FILE=config.json NODE_ID=2 ./target/release/cloud-p2p
```

**Machine 3 (192.168.1.102):**
```bash
cd ~/distributed-system
CONFIG_FILE=config.json NODE_ID=3 ./target/release/cloud-p2p
```

---

## Step 6: Verify Leader Election

Watch the output. You should see:

**On Machine 3:**
```
[Node 3] no higher nodes -> becoming coordinator
[Node 3] starting heartbeat task
```

**On Machines 1 & 2:**
```
[Node 1] Coordinator announced: 3
[Node 2] Coordinator announced: 3
```

✅ Node 3 is now the leader!

---

## Step 7: Start Client

On **any machine** (can be one of the servers or a separate machine):

```bash
cd ~/distributed-system
CONFIG_FILE=config.json ./target/release/client
```

---

## Step 8: Test Image Upload

In the client terminal:

```bash
> status
[Client] Current leader: Node 3

> send-image /path/to/your/image.jpg
[Client] ✓ Sent image abc123... to leader (node 3) — 1048576 bytes
[Client] ✓ SUCCESS: Image abc123... was accepted by the server
```

Verify on **Machine 3** (the leader):
```bash
ls -lh ~/images/
# You should see your uploaded image!
```

---

## Quick Command Reference

### Test Connectivity
```bash
# From any machine to another
telnet 192.168.1.100 7001
telnet 192.168.1.101 7002
telnet 192.168.1.102 7003
```

### Check Firewall
**Linux:**
```bash
sudo ufw status
```

**macOS:**
```bash
# Check if firewall is enabled
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --getglobalstate
```

### View Logs
```bash
# Redirect to file when starting
CONFIG_FILE=config.json NODE_ID=1 ./target/release/cloud-p2p 2>&1 | tee node1.log
```

### Stop Server
```bash
# Press Ctrl+C
```

---

## Troubleshooting

### Connection Refused
- Check firewall: `sudo ufw status`
- Verify IPs in config.json
- Test: `telnet <ip> <port>`

### Images Not Saving
- Check directory exists: `ls ~/images`
- Check permissions: `chmod 755 ~/images`
- Verify disk space: `df -h`

### Can't Find Leader
- Ensure all 3 servers are running
- Check server logs for errors
- In client, try: `discover`

---

## Summary

That's it! You now have:
- ✅ 3 servers running across the network
- ✅ Automatic leader election (Node 3)
- ✅ Client sending images to the leader
- ✅ Images stored on leader's local filesystem
- ✅ Encrypted communication between all nodes

**Note:** Images are stored locally on whichever machine is currently the leader. If the leader changes, new images will be stored on the new leader's machine.