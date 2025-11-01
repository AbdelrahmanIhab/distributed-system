# Client-Server Architecture - Milestone Update

## Overview

This distributed system now includes a **dedicated client** that communicates with a 3-server cluster. The client sends images to the **current leader**, which handles storage through load balancing and saves images to a **central shared directory**.

## Architecture

```
┌─────────┐
│ Client  │ (Interactive REPL)
└────┬────┘
     │ Discovers leader
     │ Sends images to leader
     ▼
┌────────────────────────────────────┐
│   Distributed Server Cluster       │
│                                    │
│  ┌─────────┐  ┌─────────┐  ┌───────────┐
│  │ Node 1  │  │ Node 2  │  │  Node 3   │
│  │         │  │         │  │ (Leader)  │
│  └─────────┘  └─────────┘  └─────┬─────┘
│                                   │
│       Bully Algorithm             │ Saves to
│       Leader Election             │ shared dir
└───────────────────────────────────┘
                                    │
                                    ▼
                          ┌──────────────────┐
                          │  Shared Storage  │
                          │   /shared/images │
                          └──────────────────┘
```

## Key Features

### 1. **Client Binary**
- **File**: [src/bin/client.rs](src/bin/client.rs)
- **Purpose**: External program that connects to the distributed system
- **Capabilities**:
  - Leader discovery (queries all servers, identifies current leader)
  - Interactive REPL for sending images
  - Receives confirmation messages from the server

### 2. **Leader Discovery**
- Client queries all 3 servers
- Uses Bully algorithm heuristic (highest responding node ID is likely leader)
- Can rediscover leader if it changes

### 3. **Client-Server Protocol**
- **Client → Leader**: Sends `EncryptRequest` with image data
- **Leader → Client**: Responds with `EncryptReply` (success/failure)
- Uses existing TCP + AES-256-GCM encryption

### 4. **Central Image Storage**
- **Configuration**: `shared_dir` in [config.json](config.json)
- **Default**: `/shared/images` (or `./shared_images` for local testing)
- **Behavior**: Leader saves all received images to this directory
- **File naming**: `{req_id}_{client_addr}_{timestamp}.{ext}`

### 5. **Load Balancing**
- **Strategy**: Client always sends to the current leader
- **Leader-side**: All images stored locally in shared filesystem
- **Future**: Can distribute storage tasks to followers

## Configuration

### config.json
```json
{
  "peers": [
    {"id": 1, "address": "127.0.0.1:7001"},
    {"id": 2, "address": "127.0.0.1:7002"},
    {"id": 3, "address": "127.0.0.1:7003"}
  ],
  "shared_dir": "./shared_images"
}
```

### Environment Variables
- `SHARED_DIR`: Override shared directory path (servers)
- `CONFIG_FILE`: Path to configuration file
- `NODE_ID`: Server node identifier (1, 2, or 3)

## Usage

### Quick Start (Local Testing)

```bash
# Run the automated test script
./run_local_test.sh
```

This will open 4 terminals:
- **Terminal 1-3**: Server nodes (with automatic leader election)
- **Terminal 4**: Client (interactive REPL)

### Manual Start

#### Start Servers
```bash
# Terminal 1
SHARED_DIR=./shared_images NODE_ID=1 cargo run --release --bin cloud-p2p

# Terminal 2
SHARED_DIR=./shared_images NODE_ID=2 cargo run --release --bin cloud-p2p

# Terminal 3
SHARED_DIR=./shared_images NODE_ID=3 cargo run --release --bin cloud-p2p
```

#### Start Client
```bash
# Terminal 4
cargo run --release --bin client
```

### Client Commands

Once the client is running, you can use these commands:

```
send-image <path>        # Send image to the current leader
discover                 # Rediscover the current leader
status                   # Show which node is the current leader
help                     # Show help message
```

### Example Session

```
[Client] Starting distributed system client...
[Client] Available servers: {1: 127.0.0.1:7001, 2: 127.0.0.1:7002, 3: 127.0.0.1:7003}
[Client] Discovering leader...
[Client] Leader discovered: Node 3
[Client] REPL ready — commands:
  send-image <path>        Send image to leader
  discover                 Rediscover leader
  status                   Show current leader
  help                     Show this help

> status
[Client] Current leader: Node 3

> send-image /path/to/photo.jpg
[Client] ✓ Sent image 3f2a1b4c5d6e7f8a to leader (node 3) — 524288 bytes
[Client] ✓ SUCCESS: Image 3f2a1b4c5d6e7f8a was accepted by the server
```

## Implementation Details

### Message Flow

1. **Client Startup**:
   - Loads server configuration
   - Discovers leader by sending `Hello` messages
   - Starts listening for replies on port 7100

2. **Sending an Image**:
   ```
   Client                            Leader (Node 3)
      │                                   │
      │──── EncryptRequest (image) ────▶ │
      │                                   │ (saves to shared_dir)
      │                                   │
      │◀─── EncryptReply (success) ───── │
      │                                   │
   ```

3. **Leader Storage**:
   - Leader receives `EncryptRequest`
   - Detects file format (PNG/JPG)
   - Generates unique filename
   - Saves to `shared_dir`
   - Sends confirmation back to client

### Files Modified/Created

#### New Files
- [src/bin/client.rs](src/bin/client.rs) - Client binary implementation
- [src/lib.rs](src/lib.rs) - Library exports for client
- [config.json](config.json) - Local testing configuration
- [CLIENT_README.md](CLIENT_README.md) - This documentation

#### Modified Files
- [src/message.rs](src/message.rs) - Added `QueryLeader` and `LeaderInfo` messages
- [src/config.rs](src/config.rs) - Added `shared_dir` configuration field
- [src/main.rs](src/main.rs) - Updated to use shared directory, handle client connections
- [Cargo.toml](Cargo.toml) - Added library and client binary configuration
- [run_local_test.sh](run_local_test.sh) - Updated to start client terminal

### Network Ports
- **Servers**: 7001, 7002, 7003 (configurable)
- **Client listener**: 7100 (for receiving replies)

## Testing

### Verify Leader Election
1. Start all 3 servers
2. Watch the terminal output - Node 3 (highest ID) should become leader
3. Check for "Coordinator announced" messages

### Test Image Upload
1. Start servers + client
2. In client terminal: `send-image /path/to/test.jpg`
3. Check `./shared_images/` directory for the saved file
4. Verify filename format: `{req_id}_{addr}_{timestamp}.jpg`

### Test Leader Failover
1. Start all servers + client
2. Send an image (should go to Node 3)
3. Kill Node 3
4. In client: `discover` (should find new leader, Node 2)
5. Send another image (should go to Node 2)

## Future Enhancements

- **Distributed Storage**: Leader forwards images to followers for replication
- **HTTP API**: Add REST API alongside TCP for easier client integration
- **Authentication**: Add client authentication/authorization
- **Image Processing**: Process images before storage (resize, compress, etc.)
- **Metadata Database**: Track image metadata in a database
- **Health Checks**: Client periodically pings leader to detect failures faster

## Troubleshooting

### Client can't find leader
- **Check**: All servers are running
- **Check**: Config file has correct addresses
- **Solution**: Run `discover` command in client

### Images not saving
- **Check**: `shared_dir` exists and is writable
- **Check**: Server logs for "Failed to save image" errors
- **Solution**: Create directory or fix permissions

### Connection refused
- **Check**: Server ports (7001-7003) are not blocked by firewall
- **Check**: Servers are bound to correct addresses
- **Solution**: Check firewall settings or use `BIND` env var

## Summary of Changes

### What Changed
✅ Added dedicated client binary
✅ Implemented leader discovery mechanism
✅ Created interactive client REPL
✅ Added shared filesystem configuration
✅ Updated servers to save images to shared directory
✅ Modified test script to include client
✅ Added configuration file for local testing

### Client-Server Relationship
- **Client**: Sends images via interactive commands
- **Leader**: Receives and stores images in central location
- **Protocol**: Encrypted TCP with confirmation messages
- **Storage**: Shared filesystem accessible to all nodes

This milestone establishes the foundation for a client-server architecture where external clients can submit images to a distributed system with automatic leader election and centralized storage.