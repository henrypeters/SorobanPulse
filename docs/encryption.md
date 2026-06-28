# Event Encryption at Rest

SorobanPulse supports optional AES-256-GCM encryption of the `event_data` field before it is written to the database (issue #584).

## What is encrypted

Only the `event_data` JSON column is encrypted. Indexed fields used for querying (`contract_id`, `tx_hash`, `ledger`, `event_type`, `topic`, `tenant_id`, etc.) remain in plaintext so normal queries continue to work without decryption at the query layer.

Each encrypted value is stored as a JSON envelope:

```json
{"encrypted": true, "data": "<base64-ciphertext>", "nonce": "<base64-nonce>"}
```

## Prerequisites

Build with the `encryption` feature flag:

```bash
cargo build --release --features encryption
```

The feature enables the `aes-gcm` dependency. When the feature is disabled all encrypt/decrypt calls are no-ops (plaintext pass-through).

## Configuration

### Generate a key

```bash
openssl rand -hex 32
```

This produces a 64-character hex string representing a 32-byte key.

### Environment variables

| Variable | Required | Description |
|---|---|---|
| `EVENT_DATA_ENCRYPTION_KEY` | Yes (to enable encryption) | 64-char hex AES-256 key |
| `EVENT_DATA_ENCRYPTION_KEY_OLD` | Only during key rotation | Previous key, kept until re-encryption completes |

Add these to your `.env` or Kubernetes secret:

```bash
EVENT_DATA_ENCRYPTION_KEY=aabbcc...  # 64 hex chars
```

## How it works

1. **Write path** — The indexer calls `encryption::encrypt(key, &event_data)` before inserting each row. The ciphertext envelope replaces the plaintext `event_data` value in the database.
2. **Read path** — Handlers call `encryption::decrypt(key, old_key, &event_data)` after fetching rows. Non-encrypted rows (envelope missing `"encrypted": true`) are passed through unchanged, allowing gradual migration.
3. **SSE / WebSocket streams** — Decryption is applied to each event before it is broadcast to subscribers.

## Key rotation

Rotating the encryption key is a two-step process with zero downtime.

### Step 1 — Add the new key alongside the old one

```bash
EVENT_DATA_ENCRYPTION_KEY=<new-64-hex>
EVENT_DATA_ENCRYPTION_KEY_OLD=<old-64-hex>
```

Restart the service. New events are encrypted with the new key. Existing events encrypted with the old key are still readable because `decrypt()` falls back to `EVENT_DATA_ENCRYPTION_KEY_OLD` when the primary key fails.

### Step 2 — Re-encrypt existing events

Trigger the background re-encryption job via the admin API:

```bash
curl -X POST https://your-host/v1/admin/reencrypt \
  -H "Authorization: Bearer $ADMIN_API_KEY"
```

The job fetches all rows where `event_data->>'encrypted' = 'true'`, decrypts each with the old key, and re-encrypts with the new key in configurable batches. Progress is tracked in the `soroban_pulse_reencrypt_rows_remaining` Prometheus metric.

### Step 3 — Remove the old key

Once the job completes (metric reaches 0), remove `EVENT_DATA_ENCRYPTION_KEY_OLD` and restart.

## Encrypted query support

All query handlers transparently decrypt `event_data` on read. No changes to query parameters are required. Filters on indexed columns (`contract_id`, `ledger`, etc.) work without modification because those columns are never encrypted.

## Verifying encryption

You can confirm rows are encrypted by querying the database directly:

```sql
SELECT id, event_data->>'encrypted' AS is_encrypted
FROM events
LIMIT 5;
```

Encrypted rows return `'true'`; plaintext rows return `NULL`.

## Security notes

- Each row uses a unique random 12-byte nonce. Nonce reuse is probabilistically impossible.
- The authentication tag provided by AES-256-GCM detects any tampering with stored ciphertext.
- Keys are never logged or exposed in metrics.
- Use a secrets manager (Kubernetes Secrets, AWS Secrets Manager, HashiCorp Vault) to inject `EVENT_DATA_ENCRYPTION_KEY` at runtime rather than committing it to environment files.
