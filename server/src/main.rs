// SPDX-License-Identifier: MIT
// Copyright (c) 2024 Praxis Contributors

//! Praxis server entry point.
//!
//! Loads configuration, initializes tracing (with optional JSON output and
//! per-module log level overrides), and delegates to [`praxis::run_server`].
//!
//! [`praxis::run_server`]: praxis::run_server

/// Jemalloc global allocator is used by default on unix platforms.
///
/// Reduces allocator contention under concurrent load.
#[cfg(unix)]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod commands;
mod dump;
/// Starter-config generation helpers for `praxis init`.
mod init;

use std::io::Write as _;

use clap::{Parser, Subcommand, ValueEnum};
use tracing::info;

// -----------------------------------------------------------------------------
// CLI
// -----------------------------------------------------------------------------

/// Cloud and AI-native proxy server.
#[derive(Parser)]
#[command(name = "praxis")]
struct Cli {
    /// Path to the YAML configuration file.
    #[arg(short = 'c', long = "config")]
    config: Option<String>,

    /// Dump effective configuration as YAML and exit.
    #[arg(short = 'T', long = "dump", conflicts_with = "validate")]
    dump: bool,

    /// Validate configuration and exit.
    #[arg(short = 't', long = "validate")]
    validate: bool,

    /// Generate a starter config for a common Praxis workflow.
    #[command(subcommand)]
    command: Option<Command>,
}

/// Top-level `praxis` subcommands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Generate a starter config from a curated recipe.
    Init {
        /// Incoming API family or traffic shape.
        family: InitFamily,
        /// Starter recipe within the selected family.
        recipe: InitRecipe,
    },
}

/// Supported starter-config families for `praxis init`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum InitFamily {
    /// Anthropic Messages API starters.
    Messages,
    /// OpenAI Responses API starters.
    Responses,
}

/// Supported starter recipes for `praxis init`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum InitRecipe {
    /// Native Messages protocol starter for `/v1/messages` backends.
    Protocol,
    /// Transform Messages traffic to a Chat Completions-style backend.
    ToOpenai,
    /// Minimal mode-aware routing for Responses traffic.
    Passthrough,
    /// End-to-end Responses pipeline with validation and response storage.
    FullFlow,
}

// -----------------------------------------------------------------------------
// Main
// -----------------------------------------------------------------------------

/// Entry point.
#[expect(clippy::print_stderr, reason = "fatal error output")]
fn main() {
    let cli = Cli::parse();
    if let Some(Command::Init { family, recipe }) = cli.command {
        handle_init_command(family, recipe);
        return;
    }

    let explicit = cli.config.or_else(|| std::env::var("PRAXIS_CONFIG").ok());

    if cli.validate {
        if let Err(e) = commands::load_and_validate_for_cli(explicit.as_deref()) {
            eprintln!("invalid configuration: {e}");
            std::process::exit(1);
        }
        return;
    }

    if cli.dump {
        if let Err(e) = commands::run_dump(explicit.as_deref()) {
            eprintln!("dump failed: {e}");
            std::process::exit(1);
        }
        return;
    }

    let config_path = praxis::resolve_config_path(explicit.as_deref());
    let config = praxis::load_config(explicit.as_deref()).unwrap_or_else(|e| praxis::fatal(&e));
    praxis::init_tracing(&config).unwrap_or_else(|e| praxis::fatal(&e));
    info!("starting server");
    praxis::run_server(config, config_path)
}

/// Execute `praxis init` for a selected starter family and recipe.
fn handle_init_command(family: InitFamily, recipe: InitRecipe) {
    let cwd = std::env::current_dir().unwrap_or_else(|e| praxis::fatal(&e));
    match init::run_init(&cwd, family, recipe) {
        Ok(summary) => {
            std::io::stdout()
                .write_all(summary.as_bytes())
                .and_then(|()| std::io::stdout().write_all(b"\n"))
                .unwrap_or_else(|e| praxis::fatal(&e));
        },
        Err(e) => praxis::fatal(&format!("init failed: {e}")),
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
#[expect(clippy::allow_attributes, reason = "blanket test suppressions")]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, reason = "tests")]
mod tests {
    use clap::Parser as _;

    use super::{Cli, Command, InitFamily, InitRecipe};

    // -------------------------------------------------------------------------
    // --validate CLI parsing
    // -------------------------------------------------------------------------

    #[test]
    fn cli_validate_short_flag() {
        let cli = Cli::parse_from(["praxis", "-t"]);
        assert!(cli.validate, "-t should set validate to true");
        assert!(cli.config.is_none(), "config should be None");
    }

    #[test]
    fn cli_validate_long_flag() {
        let cli = Cli::parse_from(["praxis", "--validate"]);
        assert!(cli.validate, "--validate should set validate to true");
    }

    #[test]
    fn cli_validate_with_config() {
        let cli = Cli::parse_from(["praxis", "-t", "-c", "custom.yaml"]);
        assert!(cli.validate, "-t should set validate to true");
        assert_eq!(cli.config.as_deref(), Some("custom.yaml"), "-c should set config path");
    }

    #[test]
    fn cli_default_no_validate() {
        let cli = Cli::parse_from(["praxis"]);
        assert!(!cli.validate, "validate should default to false");
        assert!(!cli.dump, "dump should default to false");
    }

    // -------------------------------------------------------------------------
    // --dump CLI parsing
    // -------------------------------------------------------------------------

    #[test]
    fn cli_dump_short_flag() {
        let cli = Cli::parse_from(["praxis", "-T"]);
        assert!(cli.dump, "-T should set dump to true");
        assert!(!cli.validate, "validate should remain false");
    }

    #[test]
    fn cli_dump_long_flag() {
        let cli = Cli::parse_from(["praxis", "--dump"]);
        assert!(cli.dump, "--dump should set dump to true");
    }

    #[test]
    fn cli_dump_with_config() {
        let cli = Cli::parse_from(["praxis", "-T", "-c", "custom.yaml"]);
        assert!(cli.dump, "-T should set dump to true");
        assert_eq!(cli.config.as_deref(), Some("custom.yaml"), "-c should set config path");
    }

    #[test]
    fn cli_dump_conflicts_with_validate() {
        let result = Cli::try_parse_from(["praxis", "--dump", "--validate"]);
        assert!(result.is_err(), "--dump and --validate should conflict");
    }

    #[test]
    fn cli_dump_short_conflicts_with_validate_short() {
        let result = Cli::try_parse_from(["praxis", "-T", "-t"]);
        assert!(result.is_err(), "-T and -t should conflict");
    }

    // -------------------------------------------------------------------------
    // init CLI parsing
    // -------------------------------------------------------------------------

    #[test]
    fn cli_init_responses_passthrough() {
        let cli = Cli::parse_from(["praxis", "init", "responses", "passthrough"]);
        let Some(Command::Init { family, recipe }) = cli.command else {
            unreachable!("expected init command");
        };
        assert_eq!(family, InitFamily::Responses);
        assert_eq!(recipe, InitRecipe::Passthrough);
    }

    #[test]
    fn cli_init_responses_full_flow() {
        let cli = Cli::parse_from(["praxis", "init", "responses", "full-flow"]);
        let Some(Command::Init { family, recipe }) = cli.command else {
            unreachable!("expected init command");
        };
        assert_eq!(family, InitFamily::Responses);
        assert_eq!(recipe, InitRecipe::FullFlow);
    }

    #[test]
    fn cli_init_unknown_recipe_rejected() {
        let result = Cli::try_parse_from(["praxis", "init", "responses", "unknown"]);
        assert!(result.is_err(), "unknown init recipe should fail clap parsing");
    }

    #[test]
    fn cli_init_messages_protocol() {
        let cli = Cli::parse_from(["praxis", "init", "messages", "protocol"]);
        let Some(Command::Init { family, recipe }) = cli.command else {
            unreachable!("expected init command");
        };
        assert_eq!(family, InitFamily::Messages);
        assert_eq!(recipe, InitRecipe::Protocol);
    }

    #[test]
    fn cli_init_messages_to_openai() {
        let cli = Cli::parse_from(["praxis", "init", "messages", "to-openai"]);
        let Some(Command::Init { family, recipe }) = cli.command else {
            unreachable!("expected init command");
        };
        assert_eq!(family, InitFamily::Messages);
        assert_eq!(recipe, InitRecipe::ToOpenai);
    }
}
