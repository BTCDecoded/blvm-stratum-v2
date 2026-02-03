# blvm-stratum-v2

Stratum V2 mining protocol module for blvm-node.

## Overview

This module implements a complete Stratum V2 mining protocol server for blvm-node. It provides:

- **Stratum V2 Server**: Full protocol implementation for miner connections
- **Mining Pool Management**: Miner registration, channel management, and job distribution
- **Share Validation**: Proof-of-work validation using the consensus layer
- **Block Template Generation**: Integration with node's block template generation
- **Automatic Block Submission**: Submits valid blocks to the node when found

## Architecture

The module integrates with blvm-node through the NodeAPI interface:

```
┌─────────────────┐
│   blvm-node     │
│  (Core Node)    │
└────────┬────────┘
         │ NodeAPI
         │ (get_block_template, submit_block)
         │
         ▼
┌─────────────────┐
│ blvm-stratum-v2 │
│    (Module)     │
│                 │
│ ┌─────────────┐ │
│ │ SV2 Server  │ │◄─── Stratum V2 Miners
│ └─────────────┘ │     (Encrypted Protocol)
└─────────────────┘
```

## Features

### Protocol Implementation

- **Setup Connection**: Protocol version negotiation and capability exchange
- **Mining Channels**: Multiple channels per miner with configurable difficulty
- **Job Distribution**: Automatic job distribution to all active channels
- **Share Submission**: Share validation with PoW checking
- **Error Handling**: Comprehensive error responses with proper error codes

### Mining Operations

- **Block Template Generation**: Uses node's consensus-verified template generation
- **Merkle Path Calculation**: Proper Stratum V2 merkle path from coinbase to root
- **Coinbase Splitting**: Splits coinbase at correct point for miner customization
- **Share Validation**: Validates shares against channel targets and network difficulty
- **Block Detection**: Automatically detects and submits valid blocks

### Pool Management

- **Miner Registration**: Tracks connected miners and their channels
- **Job Management**: Maintains job history with automatic cleanup
- **Connection Management**: Automatic cleanup of disconnected miners
- **Statistics**: Tracks shares, acceptance rates, and miner activity

## Installation

The module is part of the blvm-node module system. It is loaded automatically when configured in the node's module configuration.

## Configuration

Configure the module in the node's configuration:

```toml
[modules.blvm-stratum-v2]
enabled = true
listen_addr = "0.0.0.0:3333"
```

### Configuration Options

- `enabled`: Enable or disable the module (default: `false`)
- `listen_addr`: Address to listen for Stratum V2 connections (default: `0.0.0.0:3333`)

## Module Manifest

The module manifest (`module.toml`) defines:

```toml
name = "blvm-stratum-v2"
version = "0.1.0"
description = "Stratum V2 mining protocol module"
entry_point = "blvm-stratum-v2"

capabilities = [
    "read_blockchain",
    "subscribe_events",
]
```

## Protocol Messages

### Client → Server

- **SetupConnection**: Initial connection setup with protocol version and capabilities
- **OpenMiningChannel**: Request to open a mining channel with minimum difficulty
- **SubmitShares**: Submit mining shares for validation

### Server → Client

- **SetupConnectionSuccess**: Connection established successfully
- **SetupConnectionError**: Connection failed with error code and message
- **OpenMiningChannelSuccess**: Channel opened with target and max jobs
- **OpenMiningChannelError**: Channel open failed with error details
- **NewMiningJob**: New mining job with coinbase parts and merkle path
- **SetNewPrevHash**: Previous block hash update (chain reorganization)
- **SubmitSharesSuccess**: Shares accepted with last job ID
- **SubmitSharesError**: Shares rejected with error code and message

## Events

### Subscribed Events

The module subscribes to these node events:

- **BlockMined**: Block successfully mined - updates pool and notifies miners
- **BlockTemplateUpdated**: New block template available - generates and distributes jobs
- **MiningDifficultyChanged**: Mining difficulty changed - updates pool settings
- **StratumV2MessageReceived**: Message from miner - processes protocol messages

### Published Events

The module publishes these events:

- **MiningJobCreated**: New mining job created and distributed
- **ShareSubmitted**: Mining share submitted and validated

## Share Validation

Shares are validated using a two-stage process:

1. **Channel Target Validation**: Checks if share meets the channel's target difficulty (easier than network difficulty)
2. **Network Difficulty Validation**: Checks if share meets network difficulty (valid block)

Shares that pass channel validation are accepted. Shares that also pass network validation trigger automatic block submission.

## Job Management

- Each channel maintains a history of active jobs (configurable `max_jobs`, default: 10)
- Old jobs are automatically cleaned up when the limit is exceeded
- Jobs are distributed to all active channels when a new block template is available

## Connection Management

- Miners are automatically registered on successful connection setup
- Channels are opened per miner request with configurable difficulty
- Disconnected miners (no activity for 5+ minutes) are automatically removed
- Periodic cleanup runs every 5 minutes

## Error Handling

All protocol errors return appropriate error messages:

- **Error Code 1**: Setup connection failed (unsupported protocol version, etc.)
- **Error Code 2**: Channel open failed (miner not registered, invalid parameters)
- **Error Code 3**: Share submission failed (invalid job, channel not found, etc.)

## Dependencies

- `blvm-node`: Module system and NodeAPI integration
- `blvm-consensus`: Proof-of-work validation
- `blvm-protocol`: Protocol types and structures
- `tokio`: Async runtime
- `serde`: Message serialization
- `sha2`: Cryptographic hashing

## Testing

The module includes comprehensive test coverage:

- Unit tests for all core components
- Integration tests for protocol flows
- Concurrent operation tests for thread safety
- Error handling tests for edge cases

See `TEST_COVERAGE.md` for detailed test coverage information.

## License

MIT License - see LICENSE file for details.
