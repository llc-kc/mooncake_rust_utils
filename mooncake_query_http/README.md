# Mooncake Rust HTTP Client

A Rust-based HTTP client for querying Mooncake cache key existence via the HTTP metadata server.

## Overview

This tool allows you to check whether specific cache keys exist in a Mooncake storage instance by querying its HTTP metadata server. It supports both single key queries and batch queries with concurrent requests.

## Building

```bash
cd mooncake_rust_http
cargo build --release
```

The compiled binary will be located at `./target/release/mooncake_rust_http`.

## Usage

### Command-Line Options

```
Usage: mooncake_rust_http [OPTIONS]

Options:
  -h, --host <HOST>                Server host address [default: 10.15.56.196]
  -p, --port <PORT>                HTTP metadata server port [default: 8090]
  -k, --key <KEY>...               Cache key(s) to query (comma-separated or multiple -k)
  -k, --key-file <KEY_FILE>        File containing keys to query (one per line)
      --https                      Use HTTPS instead of HTTP
  -c, --concurrency <CONCURRENCY>  Maximum concurrent requests for batch queries [default: 10]
  -h, --help                       Print help information
```

## Single Key Query

Query a single cache key using default settings (host: 10.15.56.196, port: 8090):

```bash
./target/release/mooncake_rust_http --key my_cache_key
```

### Output Example

```
Connecting to Mooncake HTTP metadata server at 10.15.56.196:8090

Performing health check...
Health check passed: Server is running.

Querying cache key: 'my_cache_key'
Key 'my_cache_key' does not exist in the cache.

Result: Key NOT FOUND in the cache.
```

## Batch Query

### Query Multiple Keys (Comma-separated)

```bash
./target/release/mooncake_rust_http --key key1,key2,key3,key4,key5
```

### Query Multiple Keys (Multiple -k flags)

```bash
./target/release/mooncake_rust_http -k key1 -k key2 -k key3
```

### Query Keys from File

Create a file with one key per line:

```bash
cat > keys.txt << EOF
key1
key2
key3
key4
key5
EOF
```

Then query using the file:

```bash
./target/release/mooncake_rust_http --key-file keys.txt
```

### Combine File and Command-line Keys

```bash
./target/release/mooncake_rust_http --key-file keys.txt --key additional_key1,additional_key2
```

### Control Concurrency

For large batch queries, you can control the number of concurrent requests:

```bash
./target/release/mooncake_rust_http --key-file large_key_list.txt --concurrency 20
```

### Batch Query Output Example

```
Connecting to Mooncake HTTP metadata server at 10.15.56.196:8090

Performing health check...
Health check passed: Server is running.

Batch querying 5 key(s) with concurrency 10...

========== Query Results ==========
Total keys queried: 5
Keys existing:      0
Keys not found:     5

---------- Detailed Results ----------
  [NOT FOUND] key1
  [NOT FOUND] key2
  [NOT FOUND] key3
  [NOT FOUND] key4
  [NOT FOUND] key5
===================================

Result: Some keys are NOT FOUND in the cache.
```

## Advanced Examples

### Custom Host and Port

```bash
./target/release/mooncake_rust_http --host 192.168.1.100 --port 8080 --key my_key
```

### Query Using HTTPS

```bash
./target/release/mooncake_rust_http --https --key my_secure_key
```

### Batch Query with Custom Host and Concurrency

```bash
./target/release/mooncake_rust_http \
  --host 192.168.1.100 \
  --port 8080 \
  --key-file keys.txt \
  --concurrency 50
```

## Exit Codes

### Single Key Query
- `0` - Key exists in the cache
- `1` - Key does not exist in the cache or error occurred

### Batch Query
- `0` - All keys exist in the cache
- `1` - One or more keys are not found or error occurred

## How It Works

The client communicates with Mooncake's HTTP metadata server which provides the following endpoints:

- `GET /metadata?key=<key>` - Returns HTTP 200 if the key exists, HTTP 404 if not found
- `GET /health` - Health check endpoint to verify server status

For batch queries, the client sends multiple concurrent HTTP requests (controlled by `--concurrency` parameter) to maximize throughput while avoiding overwhelming the server.

## Dependencies

- `reqwest` - HTTP client library
- `tokio` - Async runtime
- `clap` - Command-line argument parsing
- `anyhow` - Error handling
- `futures` - Async utilities for concurrent execution

## Mooncake Server Configuration

To use this client, your Mooncake server must be started with the HTTP metadata server enabled:

```bash
mooncake_master \
  --eviction_high_watermark_ratio=0.95 \
  --rpc_address <RPC_ADDRESS> \
  --rpc_port <RPC_PORT> \
  --enable_http_metadata_server=true \
  --http_metadata_server_host <HTTP_HOST> \
  --http_metadata_server_port <HTTP_PORT> \
  --enable_metric_reporting=true \
  --metrics_port <METRICS_PORT>
```

## Performance Tips

1. **Concurrency Tuning**: For large batch queries, adjust `--concurrency` based on your network latency and server capacity. Default is 10.

2. **Key File Format**: When using `--key-file`, ensure:
   - One key per line
   - No empty lines (they are automatically filtered)
   - Keys are trimmed of leading/trailing whitespace

3. **Network Latency**: If querying a remote server with high latency, increasing concurrency can significantly improve throughput.
