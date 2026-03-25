use anyhow::{Context, Result};
use clap::Parser;
use reqwest::Client;
use reqwest::StatusCode;
use futures::future::join_all;

#[derive(Parser, Debug)]
#[command(name = "mooncake_rust_rpc")]
#[command(about = "RPC client for querying Mooncake cache key existence via HTTP bridge")]
struct Args {
    #[arg(short = 'H', long, default_value = "10.15.56.196")]
    host: String,

    #[arg(short, long, default_value_t = 50051)]
    rpc_port: u16,

    #[arg(short = 'm', long, default_value_t = 9003)]
    metrics_port: u16,

    #[arg(short, long, help = "Cache key(s) to query", num_args = 1.., value_delimiter = ',')]
    key: Vec<String>,

    #[arg(short = 'f', long, help = "File containing keys to query (one per line)")]
    key_file: Option<String>,

    #[arg(long, help = "Use HTTPS instead of HTTP")]
    https: bool,

    #[arg(short, long, default_value_t = 10, help = "Maximum concurrent requests for batch queries")]
    concurrency: usize,

    #[arg(long, help = "Use HTTP metadata endpoint instead of RPC query endpoint")]
    use_metadata: bool,
}

#[derive(Debug)]
struct KeyResult {
    key: String,
    exists: bool,
}

/// Client for querying Mooncake master
/// 
/// The mooncake_master exposes several HTTP endpoints:
/// - `/query_key?key=<key>` - Query if a cache key exists in the store (RPC bridge)
/// - `/batch_query_keys?keys=key1,key2` - Batch query multiple keys
/// - `/metadata?key=<key>` - Query metadata store (different from cache store)
/// - `/health` - Health check
/// 
/// This client uses the `/query_key` endpoint which directly queries the 
/// mooncake store's key existence via the WrappedMasterService::GetReplicaList
/// RPC method internally.
struct MooncakeRpcClient {
    client: Client,
    base_url: String,
    use_metadata: bool,
}

impl MooncakeRpcClient {
    fn new(host: &str, port: u16, use_https: bool, use_metadata: bool) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        let protocol = if use_https { "https" } else { "http" };
        let base_url = format!("{}://{}:{}", protocol, host, port);

        Ok(MooncakeRpcClient { 
            client, 
            base_url,
            use_metadata,
        })
    }

    /// Check if a key exists in the mooncake store
    /// 
    /// Uses the `/query_key` endpoint which returns:
    /// - 200 OK with replica descriptors if key exists
    /// - 404 Not Found if key doesn't exist
    async fn check_key_exists(&self, key: &str) -> Result<bool> {
        let endpoint = if self.use_metadata {
            "metadata"
        } else {
            "query_key"
        };
        let url = format!("{}/{}?key={}", self.base_url, endpoint, key);
        
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context(format!("Failed to send request to {}", url))?;

        match response.status() {
            StatusCode::OK => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            status => {
                let body = response.text().await.unwrap_or_default();
                Err(anyhow::anyhow!(
                    "Unexpected response status: {} - {}",
                    status,
                    body
                ))
            }
        }
    }

    /// Batch check keys for existence
    /// 
    /// Uses the `/batch_query_keys` endpoint for efficient batch queries
    async fn check_keys_batch(&self, keys: Vec<String>, concurrency: usize) -> Vec<KeyResult> {
        // Try to use batch endpoint first if not using metadata
        if !self.use_metadata {
            match self.batch_query_keys(&keys).await {
                Ok(results) => return results,
                Err(e) => {
                    eprintln!("Batch query failed, falling back to individual queries: {}", e);
                }
            }
        }
        
        // Fall back to individual queries
        let mut results = Vec::new();
        
        for chunk in keys.chunks(concurrency) {
            let futures: Vec<_> = chunk
                .iter()
                .map(|key| {
                    let key = key.clone();
                    async move {
                        let exists = match self.check_key_exists(&key).await {
                            Ok(exists) => exists,
                            Err(e) => {
                                eprintln!("Error checking key '{}': {}", key, e);
                                false
                            }
                        };
                        KeyResult {
                            key,
                            exists,
                        }
                    }
                })
                .collect();
            
            let chunk_results = join_all(futures).await;
            results.extend(chunk_results);
        }
        
        results
    }

    /// Use the batch_query_keys endpoint for efficient multi-key queries
    async fn batch_query_keys(&self, keys: &[String]) -> Result<Vec<KeyResult>> {
        let keys_param = keys.join(",");
        let url = format!("{}/batch_query_keys?keys={}", self.base_url, keys_param);
        
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send batch query request")?;

        let status = response.status();
        if status != StatusCode::OK {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Batch query failed with status: {} - {}",
                status,
                body
            ));
        }

        // Parse the JSON response
        let body = response.text().await.context("Failed to read response body")?;
        
        // The response format is:
        // {"success":true,"data":{"key1":{"ok":true,"values":[...]},"key2":{"ok":false,"error":"..."}}}
        let json: serde_json::Value = serde_json::from_str(&body)
            .context("Failed to parse JSON response")?;
        
        let mut results = Vec::new();
        
        if let Some(data) = json.get("data") {
            for key in keys {
                let key_data = data.get(key);
                let exists = key_data
                    .and_then(|d| d.get("ok"))
                    .and_then(|ok| ok.as_bool())
                    .unwrap_or(false);
                
                results.push(KeyResult {
                    key: key.clone(),
                    exists,
                });
            }
        }
        
        Ok(results)
    }

    async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);
        
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context(format!("Failed to send health check request to {}", url))?;

        match response.status() {
            StatusCode::OK => {
                println!("Health check passed: Server is running.");
                Ok(true)
            }
            status => {
                println!("Health check failed with status: {}", status);
                Ok(false)
            }
        }
    }

    /// Get all keys from the store (for debugging)
    #[allow(dead_code)]
    async fn get_all_keys(&self) -> Result<Vec<String>> {
        let url = format!("{}/get_all_keys", self.base_url);
        
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send get_all_keys request")?;

        if response.status() != StatusCode::OK {
            return Err(anyhow::anyhow!(
                "get_all_keys failed with status: {}",
                response.status()
            ));
        }

        let body = response.text().await?;
        let keys: Vec<String> = body
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        
        Ok(keys)
    }
}

fn read_keys_from_file(path: &str) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)
        .context(format!("Failed to read key file: {}", path))?;
    
    let keys: Vec<String> = content
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    
    Ok(keys)
}

fn print_results(results: &[KeyResult]) {
    let total = results.len();
    let existing = results.iter().filter(|r| r.exists).count();
    let not_found = total - existing;
    
    println!("\n========== Query Results ==========");
    println!("Total keys queried: {}", total);
    println!("Keys existing:      {}", existing);
    println!("Keys not found:     {}", not_found);
    
    if total > 1 {
        println!("\n---------- Detailed Results ----------");
        for result in results {
            let status = if result.exists { "EXISTS" } else { "NOT FOUND" };
            println!("  [{}] {}", status, result.key);
        }
    }
    println!("===================================");
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Collect all keys from both command line and file
    let mut all_keys = args.key.clone();
    
    if let Some(key_file) = args.key_file {
        let file_keys = read_keys_from_file(&key_file)
            .context("Failed to read keys from file")?;
        all_keys.extend(file_keys);
    }

    if all_keys.is_empty() {
        eprintln!("Error: No keys specified. Use --key or --key-file to provide keys.");
        std::process::exit(1);
    }

    // Determine which port to use
    // The RPC service HTTP endpoints (/query_key, /batch_query_keys, /health, /metrics)
    // are served on the metrics_port (default 9003)
    // The HTTP metadata server (/metadata) is served on http_metadata_server_port (default 8090)
    let port = if args.use_metadata {
        8090  // HTTP metadata server port
    } else {
        args.metrics_port  // RPC service HTTP port
    };

    println!("Connecting to Mooncake server at {}:{}", args.host, port);
    println!("Mode: {}", if args.use_metadata { 
        "HTTP Metadata Store" 
    } else { 
        "RPC Query (Store)" 
    });
    
    let client = MooncakeRpcClient::new(&args.host, port, args.https, args.use_metadata)
        .context("Failed to create Mooncake RPC client")?;

    println!("\nPerforming health check...");
    let is_healthy = client.health_check()
        .await
        .context("Health check failed")?;
    
    if !is_healthy {
        println!("Warning: Server may not be fully operational.");
    }

    let is_batch = all_keys.len() > 1;
    
    if is_batch {
        println!("\nBatch querying {} key(s) with concurrency {}...", all_keys.len(), args.concurrency);
        let results = client.check_keys_batch(all_keys, args.concurrency).await;
        
        print_results(&results);
        
        // Exit code: 0 if all keys exist, 1 if any key is missing
        let all_exist = results.iter().all(|r| r.exists);
        if all_exist {
            println!("\nResult: All keys EXIST in the cache.");
            std::process::exit(0);
        } else {
            println!("\nResult: Some keys are NOT FOUND in the cache.");
            std::process::exit(1);
        }
    } else {
        let key = &all_keys[0];
        println!("\nQuerying cache key: '{}'", key);
        
        match client.check_key_exists(key).await {
            Ok(exists) => {
                if exists {
                    println!("Key '{}' exists in the cache.", key);
                    println!("\nResult: Key EXISTS in the cache.");
                    std::process::exit(0);
                } else {
                    println!("Key '{}' does not exist in the cache.", key);
                    println!("\nResult: Key NOT FOUND in the cache.");
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}
