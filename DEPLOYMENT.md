# Multi-Machine Deployment Guide

This guide explains how to deploy the distributed system across multiple physical computers.

## Prerequisites

- 3 computers on the same network (or accessible to each other)
- Rust installed on each machine
- Network connectivity between all machines
- Firewall configured to allow TCP connections on your chosen ports

## Setup Steps

### 1. Find IP Addresses

On each computer, find its IP address:

```bash
# Linux/Mac
ip addr show
# or
ifconfig

# Windows
ipconfig
```

Example IPs for this guide:
- Computer 1: `192.168.1.10`
- Computer 2: `192.168.1.11`
- Computer 3: `192.168.1.12`

### 2. Create Configuration File

Create a `config.json` file with all peer addresses. **This file should be identical on all three machines.**

```json
{
  "peers": [
    {
      "id": 1,
      "address": "192.168.1.10:7001"
    },
    {
      "id": 2,
      "address": "192.168.1.11:7002"
    },
    {
      "id": 3,
      "address": "192.168.1.12:7003"
    }
  ]
}
```

**Replace the IP addresses with your actual machine IPs!**

### 3. Copy Project to All Machines

On each computer, clone or copy the project:

```bash
git clone <your-repo-url>
cd distributed-system
```

Or use `scp` to copy from one machine to others:

```bash
# From Computer 1 to Computer 2
scp -r distributed-system user@192.168.1.11:~/

# From Computer 1 to Computer 3
scp -r distributed-system user@192.168.1.12:~/
```

### 4. Configure Firewall (If Needed)

Ensure the ports are open on each machine:

```bash
# Ubuntu/Debian
sudo ufw allow 7001/tcp

# CentOS/RHEL
sudo firewall-cmd --permanent --add-port=7001/tcp
sudo firewall-cmd --reload
```

### 5. Build the Project

On each machine:

```bash
cd distributed-system
cargo build --release
```

### 6. Run the Nodes

#### Option A: Using Config File (Recommended)

On **Computer 1** (192.168.1.10):
```bash
NODE_ID=1 CONFIG_FILE=config.json cargo run --release
```

On **Computer 2** (192.168.1.11):
```bash
NODE_ID=2 CONFIG_FILE=config.json cargo run --release
```

On **Computer 3** (192.168.1.12):
```bash
NODE_ID=3 CONFIG_FILE=config.json cargo run --release
```

#### Option B: Using Environment Variables

On **Computer 1**:
```bash
NODE_ID=1 PEERS="1=192.168.1.10:7001,2=192.168.1.11:7002,3=192.168.1.12:7003" cargo run --release
```

On **Computer 2**:
```bash
NODE_ID=2 PEERS="1=192.168.1.10:7001,2=192.168.1.11:7002,3=192.168.1.12:7003" cargo run --release
```

On **Computer 3**:
```bash
NODE_ID=3 PEERS="1=192.168.1.10:7001,2=192.168.1.11:7002,3=192.168.1.12:7003" cargo run --release
```

## Verification

After starting all three nodes, you should see:

1. **Election messages** as nodes discover each other
2. **Coordinator announcement** when a leader is elected
3. **Heartbeat messages** from the leader
4. Each node displaying its peer configuration

Example output:
```
[Node 1] Starting on 192.168.1.10:7001
[Node 1] Peer configuration: {1: 192.168.1.10:7001, 2: 192.168.1.11:7002, 3: 192.168.1.12:7003}
[1] Listening on 192.168.1.10:7001
[RoundRobin] Starting peer selection for node 1
...
[Node 1] no higher nodes -> becoming coordinator
```

## Testing Communication

### Send an Image Between Nodes

From any node's terminal, you can send an image to another node:

```bash
send-image 2 /path/to/image.png
```

Or let the load balancer pick automatically:

```bash
send-image auto /path/to/image.png
```

### Using Environment Variables at Startup

Set `SEND_IMAGE_PATH` to automatically send an image when the node starts:

```bash
NODE_ID=1 CONFIG_FILE=config.json SEND_IMAGE_PATH=/path/to/test.png SEND_IMAGE_TO=2 cargo run --release
```

## Troubleshooting

### Connection Refused

**Problem**: Nodes can't connect to each other

**Solutions**:
- Verify IP addresses are correct
- Check firewall rules: `sudo ufw status` or `sudo firewall-cmd --list-all`
- Ensure the ports aren't already in use: `netstat -tulpn | grep 7001`
- Try pinging between machines: `ping 192.168.1.11`

### Node Not in Configuration

**Problem**: `Node X not found in peer configuration`

**Solution**: Ensure the config file includes an entry for every node ID you're trying to run.

### Wrong Network Interface

**Problem**: Server binds to wrong interface

**Solution**: Use explicit `BIND` environment variable:

```bash
NODE_ID=1 BIND=192.168.1.10:7001 CONFIG_FILE=config.json cargo run --release
```

### Different Subnets / Remote Deployment

If your computers are on different networks or cloud providers:

1. Use **public IP addresses** or **hostnames** in config.json
2. Ensure proper **port forwarding** and **security groups** are configured
3. Consider using a VPN like Tailscale or WireGuard for secure connections

Example for cloud deployment:
```json
{
  "peers": [
    {
      "id": 1,
      "address": "34.123.45.67:7001"
    },
    {
      "id": 2,
      "address": "52.234.56.78:7002"
    },
    {
      "id": 3,
      "address": "13.234.67.89:7003"
    }
  ]
}
```

## Production Considerations

1. **Shared Encryption Key**: The current code uses a hardcoded zero key. For production:
   ```rust
   let key = hex::decode(std::env::var("ENCRYPTION_KEY")?)?;
   ```

2. **TLS/SSL**: Consider adding TLS for secure communication

3. **Service Management**: Use systemd or similar to manage node processes:
   ```ini
   [Unit]
   Description=Distributed System Node 1
   After=network.target

   [Service]
   Type=simple
   User=yourusername
   WorkingDirectory=/home/yourusername/distributed-system
   Environment="NODE_ID=1"
   Environment="CONFIG_FILE=config.json"
   ExecStart=/home/yourusername/distributed-system/target/release/cloud-p2p
   Restart=always

   [Install]
   WantedBy=multi-user.target
   ```

4. **Monitoring**: Add logging and metrics collection for production monitoring

## Local Testing (Single Machine)

You can still test locally by using localhost addresses:

```json
{
  "peers": [
    {"id": 1, "address": "127.0.0.1:7001"},
    {"id": 2, "address": "127.0.0.1:7002"},
    {"id": 3, "address": "127.0.0.1:7003"}
  ]
}
```

Then run in separate terminals:
```bash
NODE_ID=1 CONFIG_FILE=config.json cargo run --release
NODE_ID=2 CONFIG_FILE=config.json cargo run --release
NODE_ID=3 CONFIG_FILE=config.json cargo run --release
```
