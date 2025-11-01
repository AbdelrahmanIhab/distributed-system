# Multi-Machine Deployment Guide

This guide explains how to deploy the distributed system across multiple computers on a network.

## Prerequisites

- 3 computers for servers (can be VMs, physical machines, or cloud instances)
- 1 computer for the client (can be one of the server machines or separate)
- All machines must be able to communicate over the network
- Rust toolchain installed on all machines
- Firewall rules configured to allow TCP traffic on ports 7001-7003

## Network Setup

### 1. Identify Machine IP Addresses

On each machine, find the IP address:

```bash
# Linux/Mac
ip addr show
# or
ifconfig

# Look for the IP address on your network interface (e.g., 192.168.1.x)
```

Example setup:
- **Machine 1** (Server Node 1): `192.168.1.100`
- **Machine 2** (Server Node 2): `192.168.1.101`
- **Machine 3** (Server Node 3): `192.168.1.102`
- **Client Machine**: Any of the above or `192.168.1.103`

### 2. Configure Firewall

On each server machine, allow incoming connections on the server port:

```bash
# Ubuntu/Debian with ufw
sudo ufw allow 7001/tcp  # On Machine 1
sudo ufw allow 7002/tcp  # On Machine 2
sudo ufw allow 7003/tcp  # On Machine 3

# CentOS/RHEL with firewalld
sudo firewall-cmd --permanent --add-port=7001/tcp  # On Machine 1
sudo firewall-cmd --reload

# Or disable firewall for testing (NOT recommended for production)
sudo ufw disable  # Ubuntu
sudo systemctl stop firewalld  # CentOS
```

### 3. Test Network Connectivity

From each machine, test connectivity to others:

```bash
# Test from Machine 1 to Machine 2
ping 192.168.1.101

# Test port connectivity
telnet 192.168.1.101 7002
# or
nc -zv 192.168.1.101 7002
```

## Deployment Steps

### Step 1: Copy Project to All Machines

On your development machine, create a release build:

```bash
cd /home/abdelrahman/distributed-system
cargo build --release
```

Copy the project to each server machine:

```bash
# Option 1: Using rsync
rsync -avz --exclude target /home/abdelrahman/distributed-system/ user@192.168.1.100:~/distributed-system/
rsync -avz --exclude target /home/abdelrahman/distributed-system/ user@192.168.1.101:~/distributed-system/
rsync -avz --exclude target /home/abdelrahman/distributed-system/ user@192.168.1.102:~/distributed-system/

# Option 2: Using scp
scp -r /home/abdelrahman/distributed-system user@192.168.1.100:~/

# Option 3: Use git (recommended)
# On each machine:
git clone <your-repo-url>
cd distributed-system
cargo build --release
```

### Step 2: Create Local Storage Directory

Each server will store images locally (no shared filesystem needed).

**On each server machine:**

```bash
# Create local images directory
mkdir -p ~/images

# Or use a system-wide directory
sudo mkdir -p /var/lib/cloud-p2p/images
sudo chown $USER:$USER /var/lib/cloud-p2p/images
```

**Note:** With this approach:
- Each server stores images in its own local directory
- Only the **leader** receives and stores images from clients
- Images are stored on whichever machine is currently the leader
- If the leader changes, new images will be stored on the new leader's machine

**For shared storage across all nodes** (optional, advanced):
- You can use NFS, GlusterFS, or cloud storage (S3, MinIO)
- See the end of this document for NFS setup instructions
- Not required for basic operation

### Step 3: Create Network Configuration File

Create `config.json` on each machine with the actual network addresses:

```bash
# On all machines, create the same config file
vim ~/distributed-system/config.json
```

**config.json:**
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

**Alternative: Use environment variables instead:**

```bash
export PEERS="1=192.168.1.100:7001,2=192.168.1.101:7002,3=192.168.1.102:7003"
export SHARED_DIR="/shared/images"
```

### Step 4: Start Server Nodes

**On Machine 1 (192.168.1.100):**

```bash
cd ~/distributed-system
CONFIG_FILE=config.json NODE_ID=1 cargo run --release --bin cloud-p2p

# Or using pre-built binary:
CONFIG_FILE=config.json NODE_ID=1 ./target/release/cloud-p2p
```

**On Machine 2 (192.168.1.101):**

```bash
cd ~/distributed-system
CONFIG_FILE=config.json NODE_ID=2 cargo run --release --bin cloud-p2p
```

**On Machine 3 (192.168.1.102):**

```bash
cd ~/distributed-system
CONFIG_FILE=config.json NODE_ID=3 cargo run --release --bin cloud-p2p
```

### Step 5: Verify Leader Election

Watch the output on all three machines. You should see:

```
[Node 3] no higher nodes -> becoming coordinator
[Node 3] starting heartbeat task
```

Or on the other nodes:

```
[Node 1] Coordinator announced: 3
[Node 2] Coordinator announced: 3
```

This confirms Node 3 (highest ID) became the leader.

### Step 6: Start Client

On any machine (can be one of the servers or a separate machine):

```bash
cd ~/distributed-system
CONFIG_FILE=config.json cargo run --release --bin client

# The client will use the same config.json to find the servers
```

### Step 7: Test Image Upload

In the client terminal:

```bash
> status
[Client] Current leader: Node 3

> send-image /path/to/test-image.jpg
[Client] ✓ Sent image 3f2a1b4c5d6e7f8a to leader (node 3) — 524288 bytes
[Client] ✓ SUCCESS: Image 3f2a1b4c5d6e7f8a was accepted by the server
```

**Verify on the leader machine (192.168.1.102):**

```bash
ls -lh /shared/images/
# Should show the uploaded image
```

## Running as a Service (systemd)

To run servers as background services that start automatically:

**Create service file** (`/etc/systemd/system/cloud-p2p-node1.service`):

```ini
[Unit]
Description=Cloud P2P Node 1
After=network.target

[Service]
Type=simple
User=yourusername
WorkingDirectory=/home/yourusername/distributed-system
Environment="NODE_ID=1"
Environment="CONFIG_FILE=/home/yourusername/distributed-system/config.json"
ExecStart=/home/yourusername/distributed-system/target/release/cloud-p2p
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

**Enable and start the service:**

```bash
sudo systemctl daemon-reload
sudo systemctl enable cloud-p2p-node1
sudo systemctl start cloud-p2p-node1

# Check status
sudo systemctl status cloud-p2p-node1

# View logs
sudo journalctl -u cloud-p2p-node1 -f
```

Repeat for each node with appropriate NODE_ID.

## Testing Scenarios

### Test 1: Normal Operation

1. Start all 3 servers
2. Start client
3. Send multiple images
4. Verify all images are stored in `/shared/images/`

### Test 2: Leader Failure

1. Start all 3 servers (Node 3 should be leader)
2. Send an image from client (goes to Node 3)
3. Kill Node 3: `Ctrl+C` or `sudo systemctl stop cloud-p2p-node3`
4. Wait 10-15 seconds for re-election
5. In client, run `discover` to find new leader (should be Node 2)
6. Send another image (should go to Node 2)
7. Verify image is stored

### Test 3: Network Partition

1. Block traffic between Node 1 and others:
   ```bash
   # On Machine 1
   sudo iptables -A INPUT -s 192.168.1.101 -j DROP
   sudo iptables -A INPUT -s 192.168.1.102 -j DROP
   ```
2. Observe re-election behavior
3. Restore connectivity:
   ```bash
   sudo iptables -F
   ```

## Troubleshooting

### Issue: "Connection refused"

**Symptoms:** Client or servers can't connect to each other

**Solutions:**
1. Verify IP addresses are correct in `config.json`
2. Check firewall: `sudo ufw status` or `sudo firewall-cmd --list-all`
3. Test connectivity: `telnet 192.168.1.100 7001`
4. Ensure servers are running: `ps aux | grep cloud-p2p`

### Issue: "No directory" or "Permission denied"

**Symptoms:** Images not saving, permission errors

**Solutions:**
1. Verify directory exists: `ls -ld ~/images`
2. Check permissions: `ls -ld ~/images`
3. Test write access: `touch ~/images/test.txt`
4. Create directory if missing: `mkdir -p ~/images`

### Issue: "Leader keeps changing"

**Symptoms:** Frequent re-elections, unstable leader

**Solutions:**
1. Check network stability: `ping -c 100 192.168.1.100`
2. Verify heartbeat messages are being received
3. Check system load: `top` or `htop`
4. Review logs for timeout errors

### Issue: "Client can't find leader"

**Symptoms:** Leader discovery fails

**Solutions:**
1. Verify all servers are running
2. Check config.json has correct addresses
3. Run `discover` command in client multiple times
4. Check server logs for connection attempts

## Production Considerations

### Security

1. **Use proper encryption keys:**
   ```bash
   # Generate a secure key
   openssl rand -hex 32
   # Store in environment or secure config
   export ENCRYPTION_KEY="your-secure-key-here"
   ```

2. **Enable TLS/SSL** for encrypted transport (requires code changes)

3. **Implement authentication** for client connections

4. **Use firewall rules** to restrict access:
   ```bash
   sudo ufw allow from 192.168.1.0/24 to any port 7001
   ```

### Monitoring

1. **Set up logging:**
   ```bash
   # Redirect output to log file
   cargo run --release --bin cloud-p2p 2>&1 | tee /var/log/cloud-p2p-node1.log
   ```

2. **Monitor with systemd:**
   ```bash
   journalctl -u cloud-p2p-node1 -f
   ```

3. **Add health check endpoint** (requires code changes)

### Backup

1. **Regular backups of shared directory:**
   ```bash
   # Cron job for daily backup
   0 2 * * * rsync -avz /shared/images/ /backup/images/
   ```

2. **Database backup** if you add metadata tracking

### Scaling

- **Add more nodes:** Update `config.json` with new peer addresses
- **Horizontal scaling:** Add more client instances
- **Load balancing:** Use HAProxy or nginx in front of servers

## Quick Reference Commands

```bash
# Start server node
CONFIG_FILE=config.json NODE_ID=1 ./target/release/cloud-p2p

# Start client
CONFIG_FILE=config.json ./target/release/client

# Check processes
ps aux | grep cloud-p2p

# View logs
tail -f /var/log/cloud-p2p-node1.log

# Test connectivity
telnet 192.168.1.100 7001
nc -zv 192.168.1.100 7001

# Check NFS mount
showmount -e 192.168.1.100
df -h | grep /shared

# Firewall management
sudo ufw allow 7001/tcp
sudo ufw status
```

## Summary

You now have a fully distributed system running across multiple machines with:
- ✅ Leader election across the network
- ✅ Client connecting to remote servers
- ✅ Shared storage for images
- ✅ Fault tolerance (leader failover)
- ✅ Encrypted communication

For production deployment, consider adding authentication, TLS, monitoring, and proper key management.
