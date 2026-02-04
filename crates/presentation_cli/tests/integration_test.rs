//! Integration tests for CLI
//!
//! These tests verify CLI functionality without running actual commands,
//! but instead test the command parsing and structure.

#![allow(clippy::panic)] // Allow panic! in tests for clear failure messages

use std::ffi::OsString;

use clap::Parser;

// Mock CLI structure for testing (mirrors main.rs)
#[derive(Parser)]
#[command(name = "pisovereign-cli")]
#[command(author, version, about = "PiSovereign AI Assistant CLI", long_about = None)]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    Status {
        #[arg(short, long, default_value = "http://localhost:3000")]
        url: String,
    },
    Chat {
        message: String,
        #[arg(short, long, default_value = "http://localhost:3000")]
        url: String,
    },
    Command {
        input: String,
        #[arg(short, long, default_value = "http://localhost:3000")]
        url: String,
    },
    Models {
        #[arg(short, long, default_value = "http://localhost:3000")]
        url: String,
    },
}

fn parse_args(args: &[&str]) -> Result<Cli, clap::Error> {
    let os_args: Vec<OsString> = args.iter().map(OsString::from).collect();
    Cli::try_parse_from(os_args)
}

#[test]
fn cli_parses_status_command() {
    let cli = parse_args(&["pisovereign-cli", "status"]).unwrap();
    assert!(matches!(cli.command, Commands::Status { .. }));
}

#[test]
fn cli_parses_status_with_custom_url() {
    let cli = parse_args(&["pisovereign-cli", "status", "--url", "http://custom:8080"]).unwrap();
    if let Commands::Status { url } = cli.command {
        assert_eq!(url, "http://custom:8080");
    } else {
        panic!("Expected Status command");
    }
}

#[test]
fn cli_parses_chat_command() {
    let cli = parse_args(&["pisovereign-cli", "chat", "Hello, world!"]).unwrap();
    if let Commands::Chat { message, .. } = cli.command {
        assert_eq!(message, "Hello, world!");
    } else {
        panic!("Expected Chat command");
    }
}

#[test]
fn cli_parses_chat_with_custom_url() {
    let cli = parse_args(&[
        "pisovereign-cli",
        "chat",
        "Test message",
        "--url",
        "http://api:9000",
    ])
    .unwrap();
    if let Commands::Chat { message, url } = cli.command {
        assert_eq!(message, "Test message");
        assert_eq!(url, "http://api:9000");
    } else {
        panic!("Expected Chat command");
    }
}

#[test]
fn cli_parses_command_command() {
    let cli = parse_args(&["pisovereign-cli", "command", "status"]).unwrap();
    if let Commands::Command { input, .. } = cli.command {
        assert_eq!(input, "status");
    } else {
        panic!("Expected Command command");
    }
}

#[test]
fn cli_parses_command_with_custom_url() {
    let cli = parse_args(&[
        "pisovereign-cli",
        "command",
        "help",
        "-u",
        "http://server:7000",
    ])
    .unwrap();
    if let Commands::Command { input, url } = cli.command {
        assert_eq!(input, "help");
        assert_eq!(url, "http://server:7000");
    } else {
        panic!("Expected Command command");
    }
}

#[test]
fn cli_parses_models_command() {
    let cli = parse_args(&["pisovereign-cli", "models"]).unwrap();
    assert!(matches!(cli.command, Commands::Models { .. }));
}

#[test]
fn cli_parses_models_with_custom_url() {
    let cli = parse_args(&["pisovereign-cli", "models", "--url", "http://llm:5000"]).unwrap();
    if let Commands::Models { url } = cli.command {
        assert_eq!(url, "http://llm:5000");
    } else {
        panic!("Expected Models command");
    }
}

#[test]
fn cli_parses_verbose_flag() {
    let cli = parse_args(&["pisovereign-cli", "-v", "status"]).unwrap();
    assert_eq!(cli.verbose, 1);
}

#[test]
fn cli_parses_multiple_verbose_flags() {
    let cli = parse_args(&["pisovereign-cli", "-vvv", "status"]).unwrap();
    assert_eq!(cli.verbose, 3);
}

#[test]
fn cli_requires_subcommand() {
    let result = parse_args(&["pisovereign-cli"]);
    assert!(result.is_err());
}

#[test]
fn cli_chat_requires_message() {
    let result = parse_args(&["pisovereign-cli", "chat"]);
    assert!(result.is_err());
}

#[test]
fn cli_command_requires_input() {
    let result = parse_args(&["pisovereign-cli", "command"]);
    assert!(result.is_err());
}

#[test]
fn cli_status_uses_default_url() {
    let cli = parse_args(&["pisovereign-cli", "status"]).unwrap();
    if let Commands::Status { url } = cli.command {
        assert_eq!(url, "http://localhost:3000");
    } else {
        panic!("Expected Status command");
    }
}

#[test]
fn cli_chat_uses_default_url() {
    let cli = parse_args(&["pisovereign-cli", "chat", "test"]).unwrap();
    if let Commands::Chat { url, .. } = cli.command {
        assert_eq!(url, "http://localhost:3000");
    } else {
        panic!("Expected Chat command");
    }
}

#[test]
fn cli_command_uses_default_url() {
    let cli = parse_args(&["pisovereign-cli", "command", "test"]).unwrap();
    if let Commands::Command { url, .. } = cli.command {
        assert_eq!(url, "http://localhost:3000");
    } else {
        panic!("Expected Command command");
    }
}

#[test]
fn cli_models_uses_default_url() {
    let cli = parse_args(&["pisovereign-cli", "models"]).unwrap();
    if let Commands::Models { url } = cli.command {
        assert_eq!(url, "http://localhost:3000");
    } else {
        panic!("Expected Models command");
    }
}

#[test]
fn cli_chat_handles_multiword_message() {
    let cli = parse_args(&[
        "pisovereign-cli",
        "chat",
        "This is a long message with spaces",
    ])
    .unwrap();
    if let Commands::Chat { message, .. } = cli.command {
        assert_eq!(message, "This is a long message with spaces");
    } else {
        panic!("Expected Chat command");
    }
}

#[test]
fn cli_command_handles_multiword_input() {
    let cli = parse_args(&[
        "pisovereign-cli",
        "command",
        "create calendar event tomorrow",
    ])
    .unwrap();
    if let Commands::Command { input, .. } = cli.command {
        assert_eq!(input, "create calendar event tomorrow");
    } else {
        panic!("Expected Command command");
    }
}

#[test]
fn cli_short_url_flag_works() {
    let cli = parse_args(&["pisovereign-cli", "status", "-u", "http://test:4000"]).unwrap();
    if let Commands::Status { url } = cli.command {
        assert_eq!(url, "http://test:4000");
    } else {
        panic!("Expected Status command");
    }
}

#[test]
fn cli_verbosity_zero_by_default() {
    let cli = parse_args(&["pisovereign-cli", "status"]).unwrap();
    assert_eq!(cli.verbose, 0);
}
