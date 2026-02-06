//! PiSovereign CLI
//!
//! Command-line interface for administration and testing.

#![allow(clippy::print_stdout)]

use clap::{Parser, Subcommand};
use infrastructure::ApiKeyHasher;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// PiSovereign CLI
#[derive(Parser)]
#[command(name = "pisovereign-cli")]
#[command(author, version, about = "PiSovereign AI Assistant CLI", long_about = None)]
struct Cli {
    /// Verbosity level
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check system status
    Status {
        /// Server URL
        #[arg(short, long, default_value = "http://localhost:3000")]
        url: String,
    },

    /// Send a chat message
    Chat {
        /// Message to send
        message: String,

        /// Server URL
        #[arg(short, long, default_value = "http://localhost:3000")]
        url: String,
    },

    /// Execute a command
    Command {
        /// Command input
        input: String,

        /// Server URL
        #[arg(short, long, default_value = "http://localhost:3000")]
        url: String,
    },

    /// List available models
    Models {
        /// Server URL
        #[arg(short, long, default_value = "http://localhost:3000")]
        url: String,
    },

    /// Hash an API key using Argon2 for secure storage in configuration
    ///
    /// The output can be used in config.toml for secure API key storage.
    /// Example: pisovereign-cli hash-api-key sk-my-secret-key
    HashApiKey {
        /// The plaintext API key to hash
        api_key: String,

        /// Verify the hash by re-hashing and comparing (for debugging)
        #[arg(long)]
        verify: bool,
    },
}

/// Determine log filter level from verbosity count
const fn log_filter_from_verbosity(verbose: u8) -> &'static str {
    match verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    }
}

/// Format endpoint URL
fn endpoint_url(base_url: &str, path: &str) -> String {
    format!("{base_url}{path}")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Set up logging based on verbosity
    let filter = log_filter_from_verbosity(cli.verbose);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(filter))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let client = reqwest::Client::new();

    match cli.command {
        Commands::Status { url } => {
            let resp = client
                .get(endpoint_url(&url, "/ready"))
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            println!("📊 System Status:");
            println!("{}", serde_json::to_string_pretty(&resp)?);
        },

        Commands::Chat { message, url } => {
            println!("💬 Sending: {message}");

            let resp = client
                .post(endpoint_url(&url, "/v1/chat"))
                .json(&serde_json::json!({ "message": message }))
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            if let Some(response) = resp.get("message").and_then(|v| v.as_str()) {
                println!("\n🤖 Response:\n{response}");
            }

            if let Some(latency) = resp.get("latency_ms").and_then(serde_json::Value::as_u64) {
                println!("\n⏱️  Latency: {latency}ms");
            }
        },

        Commands::Command { input, url } => {
            println!("⚡ Executing: {input}");

            let resp = client
                .post(endpoint_url(&url, "/v1/commands"))
                .json(&serde_json::json!({ "input": input }))
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            if let Some(response) = resp.get("response").and_then(|v| v.as_str()) {
                println!("\n{response}");
            }
        },

        Commands::Models { url } => {
            let resp = client
                .get(endpoint_url(&url, "/v1/system/models"))
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            println!("📦 Available Models:");
            println!("{}", serde_json::to_string_pretty(&resp)?);
        },

        Commands::HashApiKey { api_key, verify } => {
            let hasher = ApiKeyHasher::new();

            match hasher.hash(&api_key) {
                Ok(hash) => {
                    println!("🔐 API Key Hash (Argon2id):");
                    println!();
                    println!("{hash}");
                    println!();
                    println!("📋 Add to config.toml:");
                    println!("   [security]");
                    println!("   api_keys = [");
                    println!("     {{ hash = \"{hash}\", user_id = \"YOUR-USER-UUID\" }}");
                    println!("   ]");

                    if verify {
                        println!();
                        match hasher.verify(&api_key, &hash) {
                            Ok(true) => println!("✅ Verification: Hash verified successfully"),
                            Ok(false) => {
                                println!("❌ Verification: Hash does NOT match (unexpected)");
                            },
                            Err(e) => println!("❌ Verification error: {e}"),
                        }
                    }
                },
                Err(e) => {
                    println!("❌ Failed to hash API key: {e}");
                    std::process::exit(1);
                },
            }
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_filter_verbosity_zero() {
        assert_eq!(log_filter_from_verbosity(0), "warn");
    }

    #[test]
    fn log_filter_verbosity_one() {
        assert_eq!(log_filter_from_verbosity(1), "info");
    }

    #[test]
    fn log_filter_verbosity_two() {
        assert_eq!(log_filter_from_verbosity(2), "debug");
    }

    #[test]
    fn log_filter_verbosity_three_or_more() {
        assert_eq!(log_filter_from_verbosity(3), "trace");
        assert_eq!(log_filter_from_verbosity(10), "trace");
    }

    #[test]
    fn endpoint_url_concatenates_correctly() {
        assert_eq!(
            endpoint_url("http://localhost:3000", "/ready"),
            "http://localhost:3000/ready"
        );
    }

    #[test]
    fn endpoint_url_handles_trailing_slash() {
        assert_eq!(
            endpoint_url("http://example.com/", "/v1/chat"),
            "http://example.com//v1/chat"
        );
    }

    #[test]
    fn endpoint_url_with_port() {
        assert_eq!(
            endpoint_url("http://api:8080", "/v1/commands"),
            "http://api:8080/v1/commands"
        );
    }

    #[test]
    fn endpoint_url_with_https() {
        assert_eq!(
            endpoint_url("https://secure.example.com", "/v1/system/models"),
            "https://secure.example.com/v1/system/models"
        );
    }
}
