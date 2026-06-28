mod client;
mod config;
mod exporter;
mod formatter;
mod query;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

use client::ApiClient;
use config::Config;
use exporter::{export_events, ExportFormat};
use formatter::{print_contracts, print_events, print_stats, Format};
use query::{
    Contract, ContractQuery, EventQuery, EventStats, EventsResponse,
};

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

/// spulse — query and analyze Soroban Pulse events from the command line.
#[derive(Parser)]
#[command(name = "spulse", version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Soroban Pulse base URL (overrides config)
    #[arg(long, env = "SPULSE_BASE_URL", global = true)]
    base_url: Option<String>,

    /// API key (overrides config)
    #[arg(long, env = "SPULSE_API_KEY", global = true)]
    api_key: Option<String>,

    /// Output format: json | csv | table
    #[arg(long, short = 'f', global = true)]
    format: Option<Format>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Query Soroban events
    Events {
        /// Filter: starting ledger
        #[arg(long, short = 's')]
        from_ledger: Option<i64>,
        /// Filter: ending ledger
        #[arg(long, short = 'e')]
        to_ledger: Option<i64>,
        /// Filter: contract ID
        #[arg(long, short = 'c')]
        contract: Option<String>,
        /// Filter: event type (e.g. contract, system)
        #[arg(long, short = 't')]
        event_type: Option<String>,
        /// Filter: transaction hash
        #[arg(long)]
        tx: Option<String>,
        /// Results per page
        #[arg(long, short = 'l', default_value_t = 25)]
        limit: u32,
        /// Page number
        #[arg(long, short = 'p', default_value_t = 1)]
        page: u32,
        /// Sort direction: asc | desc
        #[arg(long, default_value = "desc")]
        sort: String,
        /// Sort field: ledger | created_at
        #[arg(long, default_value = "ledger")]
        sort_by: String,
        /// Write output to a file
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,
        /// Fetch all pages automatically (for --output)
        #[arg(long)]
        all: bool,
        /// Max records when using --all (safety limit)
        #[arg(long, default_value_t = 10_000)]
        max: u64,
        /// Export file format when using --output
        #[arg(long, default_value = "json")]
        export_format: ExportFormat,
    },

    /// List or search indexed contracts
    Contracts {
        /// Search query
        #[arg(long, short = 's')]
        search: Option<String>,
        /// Results per page
        #[arg(long, short = 'l', default_value_t = 25)]
        limit: u32,
        /// Page number
        #[arg(long, short = 'p', default_value_t = 1)]
        page: u32,
    },

    /// Show event statistics
    Stats {
        /// Contract ID to scope stats to
        #[arg(long, short = 'c')]
        contract: Option<String>,
    },

    /// Export events to a file (auto-paginates)
    Export {
        /// Output file path (required)
        #[arg(long, short = 'o', required = true)]
        output: PathBuf,
        /// File format: json | jsonl | csv
        #[arg(long, short = 'F', default_value = "json")]
        format: ExportFormat,
        /// Filter: starting ledger
        #[arg(long)]
        from_ledger: Option<i64>,
        /// Filter: ending ledger
        #[arg(long)]
        to_ledger: Option<i64>,
        /// Filter: contract ID
        #[arg(long, short = 'c')]
        contract: Option<String>,
        /// Filter: event type
        #[arg(long, short = 't')]
        event_type: Option<String>,
        /// Maximum number of records to export
        #[arg(long, default_value_t = 100_000)]
        max: u64,
    },

    /// Manage CLI configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Set a config value
    Set {
        /// Config key (base_url, api_key, admin_api_key, default_format, default_limit, timeout_secs)
        key: String,
        /// Value to set
        value: String,
    },
    /// Get a config value
    Get {
        key: String,
    },
    /// Show all config values
    Show,
    /// Print the config file path
    Path,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {e:#}", "error:".red().bold());
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    // Load config and apply CLI overrides
    let mut cfg = Config::load()?;
    if let Some(url) = cli.base_url    { cfg.base_url = url; }
    if let Some(key) = cli.api_key     { cfg.api_key = key; }
    let format = cli.format.unwrap_or_else(|| {
        match cfg.default_format.as_str() {
            "json"  => Format::Json,
            "csv"   => Format::Csv,
            _       => Format::Table,
        }
    });

    match cli.command {
        Commands::Events {
            from_ledger, to_ledger, contract, event_type, tx,
            limit, page, sort, sort_by, output, all, max, export_format,
        } => {
            let query = EventQuery {
                from_ledger,
                to_ledger,
                contract_id: contract.clone(),
                event_type: event_type.clone(),
                tx_hash: tx,
                limit,
                page,
                sort,
                sort_by,
            };

            if let Some(path) = output {
                // Export mode: paginate automatically
                let client = ApiClient::new(&cfg)?;
                let n = export_events(
                    &client,
                    query,
                    export_format,
                    &path,
                    if all { Some(max) } else { Some(limit as u64) },
                )?;
                println!("{} {} record(s) → {}", "Exported".green().bold(), n, path.display());
            } else {
                let client = ApiClient::new(&cfg)?;
                let owned = query.to_params();
                let params: Vec<(&str, &str)> = owned.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                let resp: EventsResponse = client.get(&query.path(), &params)?;
                print_events(&resp.into_events(), format)?;
            }
        }

        Commands::Contracts { search, limit, page } => {
            let q = ContractQuery { search, limit, page };
            let client = ApiClient::new(&cfg)?;
            let owned = q.to_params();
            let params: Vec<(&str, &str)> = owned.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

            // Try to parse as a list of Contract objects
            let contracts: Vec<Contract> = client.get(q.path(), &params).unwrap_or_default();
            print_contracts(&contracts, format)?;
        }

        Commands::Stats { contract } => {
            let client = ApiClient::new(&cfg)?;
            let path = if let Some(ref id) = contract {
                format!("/v1/contracts/{id}/summary")
            } else {
                "/v1/events/stats".into()
            };
            let stats: EventStats = client.get(&path, &[])?;
            print_stats(&stats, format)?;
        }

        Commands::Export { output, format: efmt, from_ledger, to_ledger, contract, event_type, max } => {
            let query = EventQuery {
                from_ledger,
                to_ledger,
                contract_id: contract,
                event_type,
                limit: 100,
                page: 1,
                sort: "asc".into(),
                sort_by: "ledger".into(),
                ..Default::default()
            };
            let client = ApiClient::new(&cfg)?;
            let n = export_events(&client, query, efmt, &output, Some(max))?;
            println!("{} {} record(s) → {}", "Exported".green().bold(), n, output.display());
        }

        Commands::Config { action } => handle_config(action)?,
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Config subcommand handler
// ---------------------------------------------------------------------------

fn handle_config(action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Set { key, value } => {
            let mut cfg = Config::load()?;
            cfg.set(&key, &value)?;
            cfg.save()?;
            println!("{} {}={}", "Set".green().bold(), key, value);
        }
        ConfigAction::Get { key } => {
            let cfg = Config::load()?;
            println!("{}", cfg.get(&key)?);
        }
        ConfigAction::Show => {
            let cfg = Config::load()?;
            println!("{:<16} {}", "base_url",       cfg.base_url);
            println!("{:<16} {}", "api_key",        mask(&cfg.api_key));
            println!("{:<16} {}", "admin_api_key",  mask(&cfg.admin_api_key));
            println!("{:<16} {}", "default_format", cfg.default_format);
            println!("{:<16} {}", "default_limit",  cfg.default_limit);
            println!("{:<16} {}s", "timeout",       cfg.timeout_secs);
        }
        ConfigAction::Path => {
            println!("{}", Config::path()?.display());
        }
    }
    Ok(())
}

fn mask(s: &str) -> String {
    if s.is_empty() { return "(not set)".dimmed().to_string(); }
    if s.len() <= 4 { return "****".into(); }
    format!("{}****", &s[..4])
}
