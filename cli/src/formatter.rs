use anyhow::Result;
use colored::Colorize;
use comfy_table::{Cell, Color, ContentArrangement, Table};
use serde::Serialize;

use crate::query::{Contract, EventStats, SorobanEvent};

// ---------------------------------------------------------------------------
// Format selector
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, clap::ValueEnum)]
pub enum Format {
    Json,
    Csv,
    Table,
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json  => write!(f, "json"),
            Self::Csv   => write!(f, "csv"),
            Self::Table => write!(f, "table"),
        }
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

pub fn print_events(events: &[SorobanEvent], format: Format) -> Result<()> {
    match format {
        Format::Json  => print_json(events),
        Format::Csv   => print_events_csv(events),
        Format::Table => print_events_table(events),
    }
}

fn print_events_table(events: &[SorobanEvent]) {
    if events.is_empty() {
        println!("{}", "No events found.".dimmed());
        return;
    }

    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec![
        Cell::new("LEDGER").fg(Color::Cyan),
        Cell::new("CONTRACT").fg(Color::Cyan),
        Cell::new("TYPE").fg(Color::Cyan),
        Cell::new("TX HASH").fg(Color::Cyan),
        Cell::new("CLOSED AT").fg(Color::Cyan),
        Cell::new("OK").fg(Color::Cyan),
    ]);

    for e in events {
        let ok_cell = if e.in_successful_call {
            Cell::new("✓").fg(Color::Green)
        } else {
            Cell::new("✗").fg(Color::Red)
        };
        table.add_row(vec![
            Cell::new(e.ledger),
            Cell::new(truncate(&e.contract_id, 20)),
            Cell::new(&e.event_type),
            Cell::new(truncate(&e.tx_hash, 16)),
            Cell::new(truncate(&e.ledger_closed_at, 20)),
            ok_cell,
        ]);
    }

    println!("{table}");
    println!("{}", format!("  {} event(s)", events.len()).dimmed());
}

fn print_events_csv(events: &[SorobanEvent]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    wtr.write_record(["ledger", "contract_id", "event_type", "tx_hash", "ledger_closed_at", "in_successful_call", "value"])?;
    for e in events {
        wtr.write_record(&[
            e.ledger.to_string(),
            e.contract_id.clone(),
            e.event_type.clone(),
            e.tx_hash.clone(),
            e.ledger_closed_at.clone(),
            e.in_successful_call.to_string(),
            e.value.to_string(),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Contracts
// ---------------------------------------------------------------------------

pub fn print_contracts(contracts: &[Contract], format: Format) -> Result<()> {
    match format {
        Format::Json  => print_json(contracts),
        Format::Csv   => print_contracts_csv(contracts),
        Format::Table => print_contracts_table(contracts),
    }
}

fn print_contracts_table(contracts: &[Contract]) {
    if contracts.is_empty() {
        println!("{}", "No contracts found.".dimmed());
        return;
    }

    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec![
        Cell::new("CONTRACT ID").fg(Color::Cyan),
        Cell::new("EVENTS").fg(Color::Cyan),
        Cell::new("FIRST SEEN").fg(Color::Cyan),
        Cell::new("LAST SEEN").fg(Color::Cyan),
    ]);

    for c in contracts {
        table.add_row(vec![
            Cell::new(&c.contract_id),
            Cell::new(c.event_count.map(|n| n.to_string()).unwrap_or_default()),
            Cell::new(c.first_seen.as_deref().unwrap_or("-")),
            Cell::new(c.last_seen.as_deref().unwrap_or("-")),
        ]);
    }

    println!("{table}");
    println!("{}", format!("  {} contract(s)", contracts.len()).dimmed());
}

fn print_contracts_csv(contracts: &[Contract]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    wtr.write_record(["contract_id", "event_count", "first_seen", "last_seen"])?;
    for c in contracts {
        wtr.write_record(&[
            c.contract_id.clone(),
            c.event_count.map(|n| n.to_string()).unwrap_or_default(),
            c.first_seen.clone().unwrap_or_default(),
            c.last_seen.clone().unwrap_or_default(),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

pub fn print_stats(stats: &EventStats, format: Format) -> Result<()> {
    match format {
        Format::Json | Format::Csv => print_json(stats),
        Format::Table => {
            println!("{}", "── Soroban Pulse Stats ──────────────".bold());
            println!("  Total events    : {}", stats.total_events.to_string().yellow());
            println!("  Total contracts : {}", stats.total_contracts.to_string().yellow());
            if let Some(l) = stats.latest_ledger {
                println!("  Latest ledger   : {}", l.to_string().yellow());
            }
            if let Some(n) = stats.events_last_24h {
                println!("  Last 24h        : {}", n.to_string().yellow());
            }
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Generic JSON
// ---------------------------------------------------------------------------

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value)?;
    println!("{json}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}
