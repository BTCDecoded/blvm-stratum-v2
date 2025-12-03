# bllvm-stratum-v2

Stratum V2 mining protocol module for bllvm-node.

## Overview

This module provides Stratum V2 mining protocol support for bllvm-node, including:
- Stratum V2 server implementation
- Mining pool management
- Merge mining coordination
- Mining job distribution
- Full network integration (messages routed via node network layer)

## Installation

```bash
# Install via cargo
cargo install bllvm-stratum-v2

# Or install via cargo-bllvm-module
cargo install cargo-bllvm-module
cargo bllvm-module install bllvm-stratum-v2
```

## Configuration

Create a `config.toml` in the module directory:

```toml
[stratum_v2]
enabled = true
listen_addr = "0.0.0.0:3333"
pool_url = "stratum+tcp://pool.example.com:3333"
```

## Module Manifest

The module includes a `module.toml` manifest:

```toml
name = "bllvm-stratum-v2"
version = "0.1.0"
description = "Stratum V2 mining protocol module"
author = "Bitcoin Commons Team"
entry_point = "bllvm-stratum-v2"

capabilities = [
    "read_blockchain",
    "subscribe_events",
]
```

## Events

### Subscribed Events
- `BlockMined` - Block successfully mined
- `BlockTemplateUpdated` - New block template available
- `MiningDifficultyChanged` - Mining difficulty changed
- `ChainTipUpdated` - Chain tip updated (new block)

### Published Events
- `MiningJobCreated` - New mining job created
- `ShareSubmitted` - Mining share submitted
- `MergeMiningReward` - Merge mining reward received
- `MiningPoolConnected` - Connected to mining pool
- `MiningPoolDisconnected` - Disconnected from pool

## License

MIT License - see LICENSE file for details.

