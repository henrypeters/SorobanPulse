# spulse

Command-line tool for querying and analyzing Soroban Pulse events locally.

## Install

### Cargo

```bash
cargo install spulse
```

### Homebrew

```bash
brew tap soroban-pulse/tap
brew install spulse
```

### From source

```bash
git clone https://github.com/soroban-pulse/soroban-pulse
cd soroban-pulse/cli
cargo build --release
# binary at: target/release/spulse
```

## Quick start

```bash
# Point at your Soroban Pulse instance
spulse config set base_url http://localhost:3000
spulse config set api_key  your-api-key

# Query recent events
spulse events

# Filter by contract and ledger range
spulse events --contract CABC123... --from-ledger 1000000 --to-ledger 1001000

# Output as JSON
spulse events --format json

# Export everything to a file
spulse export --output events.json --from-ledger 1000000 --max 50000

# List contracts
spulse contracts

# Show stats
spulse stats
```

## Commands

### `spulse events`

| Flag | Description | Default |
|------|-------------|---------|
| `--from-ledger, -s` | Start ledger | — |
| `--to-ledger, -e` | End ledger | — |
| `--contract, -c` | Filter by contract ID | — |
| `--event-type, -t` | Filter by event type | — |
| `--tx` | Filter by transaction hash | — |
| `--limit, -l` | Results per page | 25 |
| `--page, -p` | Page number | 1 |
| `--sort` | `asc` or `desc` | `desc` |
| `--sort-by` | `ledger` or `created_at` | `ledger` |
| `--output, -o` | Write to file | stdout |
| `--all` | Auto-paginate (with `--output`) | false |
| `--max` | Max records for `--all` | 10 000 |
| `--export-format` | `json`, `jsonl`, `csv` | `json` |

### `spulse contracts`

| Flag | Description |
|------|-------------|
| `--search, -s` | Search query |
| `--limit, -l` | Results per page |
| `--page, -p` | Page number |

### `spulse stats [--contract <ID>]`

### `spulse export`

| Flag | Description | Default |
|------|-------------|---------|
| `--output, -o` | Output file (required) | — |
| `--format, -F` | `json`, `jsonl`, `csv` | `json` |
| `--from-ledger` | Start ledger | — |
| `--to-ledger` | End ledger | — |
| `--contract, -c` | Filter by contract | — |
| `--event-type, -t` | Filter by event type | — |
| `--max` | Max records | 100 000 |

### `spulse config`

```bash
spulse config show              # print all settings
spulse config path              # print config file location
spulse config set <key> <val>   # update a setting
spulse config get <key>         # read a setting
```

**Keys:** `base_url`, `api_key`, `admin_api_key`, `default_format`, `default_limit`, `timeout_secs`

## Environment variables

All config values can be overridden at runtime:

| Variable | Config key |
|----------|------------|
| `SPULSE_BASE_URL` | `base_url` |
| `SPULSE_API_KEY` | `api_key` |

## Output formats

| Format | Use case |
|--------|----------|
| `table` | Interactive terminal (default) |
| `json` | Piping to `jq` or other tools |
| `csv` | Spreadsheet import |
| `jsonl` | Streaming / log pipelines |
