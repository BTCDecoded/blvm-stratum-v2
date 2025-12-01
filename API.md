# bllvm-stratum-v2 API Documentation

## Overview

The `bllvm-stratum-v2` module provides Stratum V2 mining protocol support for bllvm-node, including mining pool management, job distribution, and share validation.

## Core Components

### `server`

Stratum V2 server implementation.

#### `StratumV2Server`

Main server for managing miners and distributing jobs.

**Methods:**

- `new(ctx: &ModuleContext, node_api: Arc<dyn NodeAPI>) -> Result<Self, StratumV2Error>`
  - Creates a new Stratum V2 server
  - Initializes pool, template generator, and merge mining coordinator

- `handle_event(event: &ModuleMessage, node_api: &dyn NodeAPI) -> Result<(), StratumV2Error>`
  - Handles node events:
    - `BlockMined` - Updates pool and notifies miners
    - `BlockTemplateUpdated` - Generates and distributes new jobs
    - `MiningDifficultyChanged` - Updates pool difficulty
    - `StratumV2MessageReceived` - Processes messages from miners

- `handle_message(data: Vec<u8>, peer_addr: String) -> Result<Option<Vec<u8>>, StratumV2Error>`
  - Handles incoming Stratum V2 messages:
    - Setup connection
    - Open mining channel
    - Submit shares

- `update_block_template(template: Block, node_api: &dyn NodeAPI) -> Result<(), StratumV2Error>`
  - Updates block template and distributes jobs to all miners

### `pool`

Mining pool management.

#### `StratumV2Pool`

Manages miners, channels, and job distribution.

**Methods:**

- `new(node_api: Arc<dyn NodeAPI>) -> Self`
  - Creates a new mining pool

- `add_miner(endpoint: String, min_difficulty: u32) -> Result<u32, StratumV2Error>`
  - Adds a miner to the pool
  - Returns channel ID

- `set_template(template: Block, node_api: &dyn NodeAPI) -> Result<(u32, Vec<(String, u32)>), StratumV2Error>`
  - Sets new block template
  - Returns job ID and list of (endpoint, channel_id) for job distribution

- `validate_share(channel_id: u32, job_id: u32, share_data: &[u8]) -> Result<bool, StratumV2Error>`
  - Validates a mining share
  - Checks share against channel target

- `calculate_channel_target(min_difficulty: u32) -> Result<Hash, StratumV2Error>`
  - Calculates target hash for a channel based on difficulty

### `template`

Block template generation.

#### `BlockTemplateGenerator`

Generates block templates for mining.

**Methods:**

- `new(node_api: Arc<dyn NodeAPI>) -> Self`
  - Creates a new template generator

- `generate_template() -> Result<Block, StratumV2Error>`
  - Generates a new block template:
    - Gets current chain tip
    - Selects transactions from mempool
    - Creates coinbase transaction
    - Builds block

### `merge_mining`

Merge mining coordination.

#### `MergeMiningCoordinator`

Coordinates merge mining for secondary chains.

**Methods:**

- `new(node_api: Arc<dyn NodeAPI>) -> Result<Self, StratumV2Error>`
  - Creates a new merge mining coordinator

- `add_secondary_chain(name: String, target_hash: Hash) -> Result<(), StratumV2Error>`
  - Adds a secondary chain for merge mining

- `get_secondary_chains() -> Vec<SecondaryChain>`
  - Returns list of secondary chains

## Messages

### Stratum V2 Message Types

- `SetupConnection` - Initial connection setup
- `SetupConnectionSuccess` - Connection established
- `SetupConnectionError` - Connection failed
- `OpenMiningChannel` - Open a mining channel
- `OpenMiningChannelSuccess` - Channel opened
- `OpenMiningChannelError` - Channel open failed
- `NewMiningJob` - New mining job
- `SetNewPrevHash` - New previous block hash
- `SubmitShares` - Submit mining shares
- `SubmitSharesSuccess` - Share accepted
- `SubmitSharesError` - Share rejected

## Events

### Subscribed Events
- `BlockMined` - Block successfully mined
- `BlockTemplateUpdated` - New block template available
- `MiningDifficultyChanged` - Mining difficulty changed
- `StratumV2MessageReceived` - Message from miner

### Published Events
- `MiningJobCreated` - New mining job created
- `ShareSubmitted` - Mining share submitted
- `MergeMiningReward` - Merge mining reward received
- `MiningPoolConnected` - Connected to mining pool
- `MiningPoolDisconnected` - Disconnected from pool

## Configuration

```toml
[stratum_v2]
enabled = true
listen_addr = "0.0.0.0:3333"
pool_url = "stratum+tcp://pool.example.com:3333"
```

## Error Handling

All methods return `Result<T, StratumV2Error>` where `StratumV2Error` can be:
- `ServerError(String)` - Server operation failed
- `ProtocolError(String)` - Protocol error
- `MiningJob(String)` - Mining job error
- `PoolError(String)` - Pool operation failed

## Examples

### Starting the Server

```rust
let server = StratumV2Server::new(&ctx, node_api).await?;
// Server handles events automatically
```

### Adding a Miner

```rust
let pool = server.pool.read().await;
let channel_id = pool.add_miner("127.0.0.1:3333".to_string(), 1).await?;
```

### Validating a Share

```rust
let pool = server.pool.read().await;
let valid = pool.validate_share(channel_id, job_id, &share_data).await?;
if valid {
    // Share is valid
}
```

