# Quick Start: Multi-Machine Network Deployment

## TL;DR - Fast Setup

### Prerequisites
- 3 machines on the same network with known IP addresses
- Rust installed on all machines
- Firewall allowing ports 7001-7003

### Step 1: Run Setup Script (on your dev machine)

```bash
./setup_multi_machine.sh
```

This will prompt you for:
- IP addresses of your 3 machines
- Storage directory path (default: `~/images`)

It generates:
- `config_network.json` - Configuration file
- `deploy_node1.sh`, `deploy_node2.sh`, `deploy_node3.sh` - Server startup scripts
- `deploy_client.sh` - Client startup script

### Step 2: Copy to All Machines

```bash
# Replace 'user' with your username and IPs with your actual IPs

# Copy project
rsync -avz --exclude target . user@192.168.1.100:~/distributed-system/
rsync -avz --exclude target . user@192.168.1.101:~/distributed-system/
rsync -avz --exclude target . user@192.168.1.102:~/distributed-system/

# Copy config
scp config_network.json user@192.168.1.100:~/distributed-system/config.json
scp config_network.json user@192.168.1.101:~/distributed-system/config.json
scp config_network.json user@192.168.1.102:~/distributed-system/config.json
```

### Step 3: On Each Server Machine

```bash
# Create storage directory
mkdir -p ~/images

# Allow firewall
sudo ufw allow 7001/tcp  # Node 1
sudo ufw allow 7002/tcp  # Node 2
sudo ufw allow 7003/tcp  # Node 3

# Build
cd ~/distributed-system
cargo build --release

# Start server (NODE_ID = 1, 2, or 3)
CONFIG_FILE=config.json NODE_ID=1 ./target/release/cloud-p2p
```

### Step 4: Start Client (any machine)

```bash
cd ~/distributed-system
CONFIG_FILE=config.json ./target/release/client
```

### Step 5: Test It!

In the client terminal:

```bash
> status
[Client] Current leader: Node 3

> send-image /path/to/your/photo.jpg
[Client] ✓ Sent image abc123... to leader (node 3) — 1048576 bytes
[Client] ✓ SUCCESS: Image abc123... was accepted by the server
```

On the leader machine (Node 3):

```bash
ls -lh ~/images/
# You should see your uploaded image!
```

---

## Example Configuration

**config.json** (same on all machines):
```json
{
  "peers": [
    {"id": 1, "address": "192.168.1.100:7001"},
    {"id": 2, "address": "192.168.1.101:7001"},
    {"id": 3, "address": "192.168.1.102:7003"}
  ],
  "shared_dir": "/home/youruser/images"
}
```

---

## Important Notes

### Local Storage (Default)
- Each server stores images in its **own local directory**
- Only the **leader** receives and stores client images
- Images are stored on whichever machine is currently the leader
- If the leader changes (failover), new images go to the new leader

**Example:**
- Node 3 is leader → images stored on Machine 3
- Node 3 crashes → Node 2 becomes leader
- New images → stored on Machine 2

### Finding Your IP Address

```bash
# Linux
ip addr show | grep "inet "

# Mac
ifconfig | grep "inet "

# Look for addresses like 192.168.1.x or 10.0.0.x
```

### Testing Connectivity

```bash
# From any machine, test connection to servers
telnet 192.168.1.100 7001
telnet 192.168.1.101 7002
telnet 192.168.1.102 7003

# Or using netcat
nc -zv 192.168.1.100 7001
```

### Firewall Issues?

```bash
# Check firewall status
sudo ufw status

# Allow ports
sudo ufw allow 7001/tcp
sudo ufw allow 7002/tcp
sudo ufw allow 7003/tcp

# Or temporarily disable for testing (NOT recommended for production!)
sudo ufw disable
```

---

## Testing Leader Failover

1. Start all 3 servers + client
2. Check leader: `status` (should be Node 3)
3. Send an image
4. On Machine 3, press `Ctrl+C` to stop the leader
5. Wait 10-15 seconds
6. In client: `discover` (should find Node 2 as new leader)
7. Send another image (goes to Node 2 now)
8. Check images:
   - Machine 3: `ls ~/images/` (first image)
   - Machine 2: `ls ~/images/` (second image)

---

## Troubleshooting

### "Connection refused"
- Check firewall: `sudo ufw status`
- Verify IPs in config.json
- Test connectivity: `telnet <ip> <port>`

### "No directory" or "Permission denied"
- Create directory: `mkdir -p ~/images`
- Check permissions: `ls -ld ~/images`

### "Leader not found"
- Ensure all 3 servers are running
- Try `discover` command multiple times
- Check server logs for errors

### Images not saving
- Check directory exists: `ls ~/images`
- Check disk space: `df -h`
- Look for error messages in server output

---

## Running as Background Service

Create `/etc/systemd/system/cloud-p2p-node1.service`:

```ini
[Unit]
Description=Cloud P2P Node 1
After=network.target

[Service]
Type=simple
User=youruser
WorkingDirectory=/home/youruser/distributed-system
Environment="NODE_ID=1"
Environment="CONFIG_FILE=config.json"
ExecStart=/home/youruser/distributed-system/target/release/cloud-p2p
Restart=always

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable cloud-p2p-node1
sudo systemctl start cloud-p2p-node1
sudo systemctl status cloud-p2p-node1
```

---

## Summary

✅ **3 servers** across the network with automatic leader election
✅ **Client** sends images to current leader
✅ **Local storage** on each server (leader stores images)
✅ **Fault tolerance** - automatic leader failover
✅ **Encrypted** TCP communication

**Next Steps:** See [DEPLOYMENT_MULTI_MACHINE.md](DEPLOYMENT_MULTI_MACHINE.md) for production deployment, monitoring, and advanced features.
