//! PiSovereign CLI
//!
//! Command-line interface for administration and testing.

use clap::{Parser, Subcommand};
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Set up logging based on verbosity
    let filter = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(filter))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let client = reqwest::Client::new();

    match cli.command {
        Commands::Status { url } => {
            let resp = client
                .get(format!("{}/ready", url))
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            println!("📊 System Status:");
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        Commands::Chat { message, url } => {
            println!("💬 Sending: {}", message);

            let resp = client
                .post(format!("{}/v1/chat", url))
                .json(&serde_json::json!({ "message": message }))
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            if let Some(response) = resp.get("message").and_then(|v| v.as_str()) {
                println!("\n🤖 Response:\n{}", response);
            }

            if let Some(latency) = resp.get("latency_ms").and_then(|v| v.as_u64()) {
                println!("\n⏱️  Latency: {}ms", latency);
            }
        }

        Commands::Command { input, url } => {
            println!("⚡ Executing: {}", input);

            let resp = client
                .post(format!("{}/v1/commands", url))
                .json(&serde_json::json!({ "input": input }))
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            if let Some(response) = resp.get("response").and_then(|v| v.as_str()) {
                println!("\n{}", response);
            }
        }

        Commands::Models { url } => {
            let resp = client
                .get(format!("{}/v1/system/models", url))
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            println!("📦 Available Models:");
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
    }

    Ok(())
}
