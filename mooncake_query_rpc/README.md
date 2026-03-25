# Mooncake Rust RPC Client

A Rust-based RPC client for querying Mooncake cache key existence via HTTP bridge.

## Overview

This client connects to the mooncake_master's HTTP bridge endpoints to check whether specific cache keys exist in the store. It supports both single key queries and batch queries with configurable concurrency.

## Features

- **Single Key Query**: Check if a specific cache key exists
- **Batch Key Query**: Check multiple keys efficiently with configurable concurrency
- **Health Check**: Verify server availability before querying
- **Two Query Modes**:
  - RPC Query mode (default): Uses `/query_key` endpoint on the metrics port
  - Metadata mode: Uses `/metadata` endpoint on the HTTP metadata server port

## Building

```bash
cd mooncake_rust_rpc
cargo build --release
```

The compiled binary will be available at `./target/release/mooncake_rust_rpc`.

## Usage

### Basic Usage

```bash
# Query a single key (connects to 10.15.56.196:9003 by default)
./target/release/mooncake_rust_rpc -k my_cache_key

# Query multiple keys
./target/release/mooncake_rust_rpc -k key1,key2,key3

# Query keys from a file (one key per line)
./target/release/mooncake_rust_rpc -f keys.txt
```

### Command Line Options

```
Options:
  -H, --host <HOST>                  Server host [default: 10.15.56.196]
  -r, --rpc-port <RPC_PORT>          RPC port [default: 50051]
  -m, --metrics-port <METRICS_PORT>  Metrics/HTTP bridge port [default: 9003]
  -k, --key <KEY>...                 Cache key(s) to query
  -f, --key-file <KEY_FILE>          File containing keys to query (one per line)
      --https                        Use HTTPS instead of HTTP
  -c, --concurrency <CONCURRENCY>    Maximum concurrent requests for batch queries [default: 10]
      --use-metadata                 Use HTTP metadata endpoint instead of RPC query endpoint
  -h, --help                         Print help
```

### Advanced Examples

#### Custom Host and Port

```bash
# Connect to a different server
./target/release/mooncake_rust_rpc -H 192.168.1.100 -m 9003 -k my_key
```

#### Batch Query with Concurrency Control

```bash
# Query 100 keys with max 20 concurrent requests
./target/release/mooncake_rust_rpc -k key1,key2,...,key100 -c 20
```

#### Using HTTP Metadata Server

```bash
# Query via HTTP metadata server (port 8090)
./target/release/mooncake_rust_rpc --use-metadata -k my_key
```

#### Query from File

Create a file `keys.txt`:
```
cache_key_1
cache_key_2
cache_key_3
```

Then run:
```bash
./target/release/mooncake_rust_rpc -f keys.txt
```

#### Combine Command Line and File Keys

```bash
./target/release/mooncake_rust_rpc -k key1,key2 -f more_keys.txt
```

### Exit Codes

- **0**: All queried keys exist in the cache
- **1**: At least one key was not found or an error occurred

### Output Examples

#### Single Key Query

```
Connecting to Mooncake server at 10.15.56.196:9003
Mode: RPC Query (Store)

Performing health check...
Health check passed: Server is running.

Querying cache key: 'my_cache_key'
Key 'my_cache_key' does not exist in the cache.

Result: Key NOT FOUND in the cache.
```

#### Batch Query

```
Connecting to Mooncake server at 10.15.56.196:9003
Mode: RPC Query (Store)

Performing health check...
Health check passed: Server is running.

Batch querying 3 key(s) with concurrency 10...

========== Query Results ==========
Total keys queried: 3
Keys existing:      0
Keys not found:     3

---------- Detailed Results ----------
  [NOT FOUND] key1
  [NOT FOUND] key2
  [NOT FOUND] key3
===================================

Result: Some keys are NOT FOUND in the cache.
```

## Architecture

The mooncake_master exposes several HTTP endpoints:

| Endpoint | Port | Description |
|----------|------|-------------|
| `/query_key?key=<key>` | 9003 (metrics_port) | Query if a cache key exists in the store |
| `/batch_query_keys?keys=k1,k2` | 9003 (metrics_port) | Batch query multiple keys |
| `/metadata?key=<key>` | 8090 (http_metadata_server_port) | Query HTTP metadata store |
| `/health` | 9003 | Health check endpoint |
| `/metrics` | 9003 | Prometheus metrics endpoint |

**Note**: The RPC Query mode (default) queries the actual cache store, while the Metadata mode queries a separate metadata store. For checking cache key existence, use the default RPC Query mode.

## Connection to Deployed Instance

Based on your mooncake_master configuration:
- RPC Address: 10.15.56.196:50061
- HTTP Metadata Server: 10.15.56.196:8090
- Metrics/HTTP Bridge: 10.15.56.196:9003

The client defaults to connecting to `10.15.56.196:9003` for RPC queries.

## Dependencies

- tokio - Async runtime
- reqwest - HTTP client
- clap - Command line argument parsing
- serde_json - JSON parsing
- futures - Async utilities
- anyhow - Error handling
- uuid - UUID generation

## License

Same as the Mooncake project.
