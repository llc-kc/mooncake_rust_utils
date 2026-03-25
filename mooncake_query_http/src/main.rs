use anyhow::{Context, Result};
use clap::Parser;
use reqwest::Client;
use reqwest::StatusCode;
use futures::future::join_all;

#[derive(Parser, Debug)]
#[command(name = "mooncake_rust_http")]
#[command(about = "HTTP client for querying Mooncake cache key existence")]
struct Args {
    #[arg(short, long, default_value = "10.15.56.196")]
    host: String,

    #[arg(short, long, default_value_t = 8090)]
    port: u16,

    #[arg(short, long, help = "Cache key(s) to query", num_args = 1.., value_delimiter = ',')]
    key: Vec<String>,

    #[arg(short, long, help = "File containing keys to query (one per line)")]
    key_file: Option<String>,

    #[arg(long, help = "Use HTTPS instead of HTTP")]
    https: bool,

    #[arg(short, long, default_value_t = 10, help = "Maximum concurrent requests for batch queries")]
    concurrency: usize,
}

#[derive(Debug)]
struct KeyResult {
    key: String,
    exists: bool,
}

struct MooncakeHttpClient {
    client: Client,
    base_url: String,
}

impl MooncakeHttpClient {
    fn new(host: &str, port: u16, use_https: bool) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        let protocol = if use_https { "https" } else { "http" };
        let base_url = format!("{}://{}:{}", protocol, host, port);

        Ok(MooncakeHttpClient { client, base_url })
    }

    async fn check_key_exists(&self, key: &str) -> Result<bool> {
        let url = format!("{}/metadata?key={}", self.base_url, key);
        
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

    async fn check_keys_batch(&self, keys: Vec<String>, concurrency: usize) -> Vec<KeyResult> {
        let mut results = Vec::new();
        
        // Process keys in chunks to limit concurrency
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

    println!("Connecting to Mooncake HTTP metadata server at {}:{}", args.host, args.port);
    
    let client = MooncakeHttpClient::new(&args.host, args.port, args.https)
        .context("Failed to create Mooncake HTTP client")?;

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
