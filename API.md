# blvm-stratum-v2 API Documentation

## Overview

The `blvm-stratum-v2` module provides a complete Stratum V2 mining protocol implementation. This document describes the public API and internal structure.

## Core Components

### Server

#### `StratumV2Server`

Main server implementation that manages miners, distributes jobs, and handles protocol messages.

**Creation:**

```rust
pub async fn new(
    ctx: &ModuleContext,
    node_api: Arc<dyn NodeAPI>,
) -> Result<Self, StratumV2Error>
```

Creates a new Stratum V2 server with the given module context and node API.

**Server Control:**

```rust
pub async fn start(&self) -> Result<(), StratumV2Error>
```

Starts the server and begins accepting connections.

```rust
pub async fn stop(&self) -> Result<(), StratumV2Error>
```

Stops the server and closes all connections.

**Event Handling:**

```rust
pub async fn handle_event(
    &self,
    event: &ModuleMessage,
    node_api: &dyn NodeAPI,
) -> Result<(), StratumV2Error>
```

Handles events from the node:
- `BlockMined`: Updates pool and notifies miners of new block
- `BlockTemplateUpdated`: Generates and distributes new jobs
- `MiningDifficultyChanged`: Updates pool difficulty settings
- `StratumV2MessageReceived`: Processes messages from miners

**Message Handling:**

```rust
pub async fn handle_message(
    &self,
    data: Vec<u8>,
    endpoint: String,
) -> Result<Vec<u8>, StratumV2Error>
```

Handles incoming Stratum V2 protocol messages and returns response. Supports:
- `SetupConnection`: Protocol version negotiation
- `OpenMiningChannel`: Channel opening with difficulty
- `SubmitShares`: Share submission and validation

**Statistics:**

```rust
pub async fn get_pool_stats(&self) -> PoolStats
```

Returns current pool statistics (miner count, shares, etc.).

**Pool Access:**

```rust
pub fn get_pool(&self) -> Arc<RwLock<StratumV2Pool>>
```

Returns a reference to the mining pool for direct access.

### Pool

#### `StratumV2Pool`

Manages miners, channels, jobs, and share validation.

**Creation:**

```rust
pub fn new() -> Self
```

Creates a new empty mining pool.

**Miner Management:**

```rust
pub fn register_miner(&mut self, endpoint: String)
```

Registers a new miner connection.

```rust
pub fn open_channel(
    &mut self,
    endpoint: &str,
    channel_id: u32,
    min_difficulty: u32,
) -> Result<Hash, StratumV2Error>
```

Opens a mining channel for a miner with the specified difficulty. Returns the channel target hash.

**Job Management:**

```rust
pub fn set_template(&mut self, template: Block) -> (u32, Vec<(String, u32)>)
```

Sets a new block template and creates a mining job. Returns the job ID and list of (endpoint, channel_id) pairs for job distribution.

**Share Handling:**

```rust
pub fn handle_share(
    &mut self,
    endpoint: &str,
    share: ShareData,
) -> Result<(bool, bool), StratumV2Error>
```

Processes a share submission. Returns `(is_valid_share, is_valid_block)`:
- `is_valid_share`: Share meets channel target
- `is_valid_block`: Share meets network difficulty (valid block)

**Cleanup:**

```rust
pub fn cleanup_old_jobs(&mut self)
```

Removes old jobs beyond the `max_jobs` limit per channel.

```rust
pub fn cleanup_disconnected_miners(&mut self, timeout_seconds: u64)
```

Removes miners with no activity for the specified timeout period.

**Validation:**

```rust
pub fn validate_share(
    &self,
    share: &ShareData,
    job_info: &JobInfo,
    target: &Hash,
) -> bool
```

Validates a share against the job and channel target. Uses consensus layer for PoW validation.

```rust
pub fn calculate_channel_target(
    &self,
    min_difficulty: u32,
) -> Result<Hash, StratumV2Error>
```

Calculates the target hash for a channel based on minimum difficulty.

```rust
pub fn calculate_block_hash(
    &self,
    header: &BlockHeader,
) -> Hash
```

Calculates block hash (double SHA256) using consensus layer serialization.

```rust
pub fn hash_less_than_or_equal(
    &self,
    hash1: &Hash,
    hash2: &Hash,
) -> bool
```

Compares two hashes as big-endian integers. Returns true if `hash1 <= hash2`.

### Template Generator

#### `BlockTemplateGenerator`

Generates block templates using the node's consensus-verified template generation.

**Creation:**

```rust
pub fn new(node_api: Arc<dyn NodeAPI>) -> Self
```

Creates a new template generator with node API access.

**Template Generation:**

```rust
pub async fn generate_template(&self) -> Result<Block, StratumV2Error>
```

Generates a new block template by calling `NodeAPI::get_block_template()` and converting the result to a `Block`.

### Protocol

#### `TlvEncoder`

Encodes Stratum V2 messages using Tag-Length-Value format.

```rust
pub fn new() -> Self
pub fn encode(&mut self, tag: u16, payload: &[u8]) -> ProtocolResult<Vec<u8>>
```

Encodes a message with the given tag and payload. Format: `[4-byte length][2-byte tag][4-byte length][payload]`

#### `TlvDecoder`

Decodes Stratum V2 messages from Tag-Length-Value format.

```rust
pub fn new(data: Vec<u8>) -> Self
pub fn decode(&mut self) -> ProtocolResult<(u16, Vec<u8>)>
pub fn decode_raw(data: &[u8]) -> ProtocolResult<(u16, Vec<u8>)>
```

Decodes a message, returning the tag and payload. `decode()` expects length-prefixed format, `decode_raw()` expects raw TLV data.

### Messages

All Stratum V2 message types implement the `StratumV2Message` trait:

```rust
pub trait StratumV2Message: Serialize + for<'de> Deserialize<'de> {
    fn message_type(&self) -> u16;
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error>;
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error>;
}
```

**Message Types:**

- `SetupConnectionMessage`: Protocol version and capabilities
- `SetupConnectionSuccessMessage`: Supported versions and capabilities
- `SetupConnectionErrorMessage`: Error code and message
- `OpenMiningChannelMessage`: Channel ID, request ID, minimum difficulty
- `OpenMiningChannelSuccessMessage`: Channel ID, request ID, target, max jobs
- `OpenMiningChannelErrorMessage`: Channel ID, request ID, error code, message
- `NewMiningJobMessage`: Channel ID, job ID, previous hash, coinbase parts, merkle path
- `SetNewPrevHashMessage`: Channel ID, job ID, previous hash, minimum time
- `SubmitSharesMessage`: Channel ID and list of shares
- `SubmitSharesSuccessMessage`: Channel ID and last job ID
- `SubmitSharesErrorMessage`: Channel ID, job ID, error code, message

### Data Structures

#### `MinerConnection`

Represents a connected miner:

```rust
pub struct MinerConnection {
    pub endpoint: String,
    pub channels: HashMap<u32, ChannelInfo>,
    pub stats: MinerStats,
}
```

#### `ChannelInfo`

Represents a mining channel:

```rust
pub struct ChannelInfo {
    pub channel_id: u32,
    pub target: Hash,
    pub current_job_id: Option<u32>,
    pub min_difficulty: u32,
    pub max_jobs: u32,
    pub jobs: HashMap<u32, JobInfo>,
}
```

#### `JobInfo`

Represents a mining job:

```rust
pub struct JobInfo {
    pub job_id: u32,
    pub prev_hash: Hash,
    pub bits: u32,
    pub timestamp: u64,
}
```

#### `ShareData`

Represents a submitted share:

```rust
pub struct ShareData {
    pub channel_id: u32,
    pub job_id: u32,
    pub nonce: u32,
    pub version: i64,
    pub merkle_root: Hash,
}
```

#### `MinerStats`

Tracks miner statistics:

```rust
pub struct MinerStats {
    pub total_shares: u64,
    pub accepted_shares: u64,
    pub rejected_shares: u64,
    pub last_share_time: Option<u64>,
}
```

#### `PoolStats`

Pool-wide statistics:

```rust
pub struct PoolStats {
    pub miner_count: usize,
    pub total_shares: u64,
    pub accepted_shares: u64,
    pub rejected_shares: u64,
}
```

## Error Types

### `StratumV2Error`

All operations return `Result<T, StratumV2Error>`:

```rust
pub enum StratumV2Error {
    ModuleError(String),
    ProtocolError(String),
    PoolConnectionError(String),
    TemplateError(String),
    ConfigError(String),
    PoolError(String),
    ServerError(String),
}
```

## Examples

### Server Initialization

```rust
let ctx = ModuleContext { /* ... */ };
let node_api = Arc::new(node_api_impl);
let server = StratumV2Server::new(&ctx, node_api).await?;
server.start().await?;
```

### Processing Shares

```rust
let share = ShareData {
    channel_id: 1,
    job_id: 1,
    nonce: 12345,
    version: 1,
    merkle_root: [0u8; 32],
};

let pool = server.get_pool().await;
let (is_valid, is_block) = pool.write().await.handle_share("miner-1", share)?;

if is_block {
    // Valid block found - automatically submitted
}
```

### Getting Statistics

```rust
let stats = server.get_pool_stats().await;
println!("Miners: {}, Shares: {}/{}", 
    stats.miner_count,
    stats.accepted_shares,
    stats.total_shares
);
```
