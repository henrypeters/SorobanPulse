# Subscription Code Generator

`gen_subscription_scaffold` generates a complete set of Rust source files, a SQL migration, and optionally content filter and test scaffolding for a new subscription type.

## Usage

```bash
cargo run --bin gen_subscription_scaffold -- <NAME> [OPTIONS]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--output-dir <path>` | `.` (current dir) | Where to write generated files |
| `--channel-type <type>` | `webhook` | `webhook`, `email`, or `sms` |
| `--with-filter` | off | Generate a content filter config module |
| `--with-tests` | off | Generate a test scaffold module |
| `--dry-run` | off | Print files to stdout instead of writing |

## Examples

```bash
# Minimal: webhook subscription named "token-transfer"
cargo run --bin gen_subscription_scaffold -- token-transfer

# Email subscription with tests
cargo run --bin gen_subscription_scaffold -- payment \
  --channel-type email \
  --with-tests

# Full scaffold with filter and tests, preview to stdout
cargo run --bin gen_subscription_scaffold -- nft-sale \
  --with-filter \
  --with-tests \
  --dry-run

# Write to a specific directory
cargo run --bin gen_subscription_scaffold -- dex-swap \
  --with-filter \
  --with-tests \
  --output-dir /tmp/scaffold-preview
```

## Generated Files

Given `NAME = token-transfer` and `--channel-type webhook`:

| File | Description |
|------|-------------|
| `src/token_transfer_subscriptions.rs` | Axum handlers: create, get, cancel, ack; delivery worker |
| `migrations/<ts>_add_token_transfer_subscriptions.sql` | Creates `token_transfer_subscriptions` + `token_transfer_delivery_queue` tables |
| `src/token_transfer_webhook.rs` | Webhook delivery with HMAC signing, retry, failover, DLQ |
| `src/token_transfer_filter_config.rs` | *(with `--with-filter`)* Filter config struct and preset bundles |
| `tests/token_transfer_subscription_tests.rs` | *(with `--with-tests`)* Unit + integration test stubs |

## Integration Steps

After generation:

1. **Register the module** in `src/lib.rs`:
   ```rust
   pub mod token_transfer_subscriptions;
   pub mod token_transfer_webhook;          // for webhook channel
   pub mod token_transfer_filter_config;   // if --with-filter was used
   ```

2. **Register routes** in `src/routes.rs`:
   ```rust
   use crate::token_transfer_subscriptions::*;

   // Inside your router builder:
   .route("/subscriptions/token-transfer",      post(create_token_transfer_subscription))
   .route("/subscriptions/token-transfer/:id",  get(get_token_transfer_subscription))
   .route("/subscriptions/token-transfer/:id",  delete(cancel_token_transfer_subscription))
   .route("/subscriptions/token-transfer/:id/ack", post(ack_token_transfer_subscription))
   ```

3. **Apply the migration**:
   ```bash
   sqlx migrate run
   ```

4. **Start the delivery worker** alongside your main server:
   ```rust
   tokio::spawn(run_token_transfer_delivery_worker(pool.clone(), http_client.clone()));
   ```

5. **Wire content filters** (if generated):
   ```rust
   use crate::token_transfer_filter_config::{TokenTransferFilterConfig, evaluate_all};

   // In your delivery worker, before posting to callback_url:
   if !evaluate_all(&active_filters, &event_data) {
       continue; // skip this event
   }
   ```

6. **Run tests**:
   ```bash
   cargo test token_transfer            # unit tests
   make test-db                         # integration tests (requires PostgreSQL)
   ```

## How It Works

The generator lives in `src/codegen/`:

| Module | Purpose |
|--------|---------|
| `mod.rs` | `ScaffoldConfig`, `ChannelType`, `generate_all()`, `write_files()` |
| `subscription.rs` | Handler + migration templates |
| `filter.rs` | Content filter config template |
| `webhook.rs` | Webhook, email, or SMS delivery template (selected by `--channel-type`) |
| `tests.rs` | Test scaffold template |

Templates use `{{PASCAL}}` / `{{SNAKE}}` / `{{CHANNEL}}` placeholder substitution via simple `str::replace`, avoiding any external template engine dependency.

## Design Decisions

- **No new dependencies**: uses `str::replace` for templating instead of Handlebars or Tera.
- **Templates match existing conventions**: generated code follows the same patterns as `subscriptions.rs` and `webhook.rs`.
- **SQL mirrors the base schema**: generated tables parallel `subscriptions` and `delivery_queue` with the same columns and index strategy.
- **Exponential backoff is preserved**: generated retry SQL uses `LEAST(POWER(2, attempts + 1), 3600)` — identical to the base worker.
- **SSRF protection reused**: generated handlers call `validate_callback_url()` from `crate::subscriptions` rather than duplicating the logic.
