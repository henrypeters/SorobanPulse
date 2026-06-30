# Contract Event Schemas

Reference for Soroban contract event patterns and their XDR representations.

## Event Structure

Every event emitted by a Soroban contract is captured by SorobanPulse with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `contract_id` | `string` | Strkey-encoded contract address (`C…`) |
| `event_type` | `string` | `contract`, `system`, or `diagnostic` |
| `tx_hash` | `string` | Hex-encoded transaction hash |
| `ledger` | `integer` | Ledger sequence number |
| `ledger_closed_at` | `string` | RFC 3339 timestamp of ledger close |
| `ledger_hash` | `string \| null` | Hex-encoded ledger hash (when tracked) |
| `in_successful_call` | `boolean` | Whether the event came from a successful invocation |
| `value` | `object` | Decoded event data (JSON) |
| `topic` | `array \| null` | Array of XDR-encoded topic segments |
| `tenant_id` | `string \| null` | Tenant identifier (multi-tenant deployments) |

## Common Contract Event Patterns

### Transfer Event

Emitted by SEP-41 compliant token contracts when tokens move between accounts.

**Topics:**
```
[Symbol("transfer"), Address(from), Address(to)]
```

**Data:**
```json
{
  "amount": "1000000000",
  "asset": "native"
}
```

**Full example:**
```json
{
  "contract_id": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
  "event_type": "contract",
  "tx_hash": "3389e9f0f1a65f19736cacf544c2e825313e8447f569233bb8db39aa607c8889",
  "ledger": 5432100,
  "ledger_closed_at": "2026-06-29T12:00:00Z",
  "in_successful_call": true,
  "value": {
    "amount": "1000000000",
    "asset": "native"
  },
  "topic": [
    "AAAADwAAAAh0cmFuc2Zlcg==",
    "AAAABQAAAAAAAAAA...",
    "AAAABQAAAAAAAAAB..."
  ]
}
```

### Mint Event

Emitted when new tokens are minted.

**Topics:**
```
[Symbol("mint"), Address(admin), Address(to)]
```

**Data:**
```json
{
  "amount": "500000000"
}
```

### Burn Event

Emitted when tokens are destroyed.

**Topics:**
```
[Symbol("burn"), Address(from)]
```

**Data:**
```json
{
  "amount": "250000000"
}
```

### Approve Event

Emitted when a spender allowance is set.

**Topics:**
```
[Symbol("approve"), Address(owner), Address(spender)]
```

**Data:**
```json
{
  "amount": "10000000000",
  "expiration_ledger": 5532100
}
```

## XDR Format

Soroban contract events use XDR (External Data Representation) to encode topics and data. SorobanPulse decodes these into JSON but also exposes the raw base64-encoded XDR in the `topic` array.

### ScVal Types

The most common XDR `ScVal` types and their JSON equivalents:

| XDR Type | JSON representation |
|----------|---------------------|
| `ScvSymbol` | `"string_value"` |
| `ScvI128` | `"123456789"` (string to avoid precision loss) |
| `ScvU128` | `"123456789"` |
| `ScvBool` | `true` / `false` |
| `ScvAddress` | Strkey string (`G…` or `C…`) |
| `ScvBytes` | Base64-encoded string |
| `ScvMap` | JSON object |
| `ScvVec` | JSON array |

### Decoding Topics Manually

Topics are base64-encoded XDR `ScVal` values. To decode them:

```bash
# Decode a single topic segment
echo "AAAADwAAAAh0cmFuc2Zlcg==" | base64 -d | xxd

# Using the Stellar SDK (JavaScript)
const xdr = StellarSdk.xdr;
const val = xdr.ScVal.fromXDR(Buffer.from(base64Topic, 'base64'));
console.log(val.value().toString());  // "transfer"
```

```rust
// Using stellar-xdr crate
use stellar_xdr::curr::{ScVal, ReadXdr};

let bytes = base64::decode(topic_b64).unwrap();
let val = ScVal::from_xdr_base64(topic_b64, stellar_xdr::curr::Limits::none()).unwrap();
```

### Topic Filtering

SorobanPulse indexes `topic[0]` as `topic_0_sym` for fast equality filtering. Use the `topic_0` query parameter to filter by event name:

```bash
# Fetch all transfer events for a contract
GET /v1/events?contract_id=C...&topic_0=transfer

# Fetch all mint and burn events
GET /v1/events?contract_id=C...&topic_0=mint
GET /v1/events?contract_id=C...&topic_0=burn
```

## System Events

System events are emitted by the Stellar network itself rather than user contracts.

| `event_type` | Trigger |
|---|---|
| `system` | Protocol-level operations (e.g., fee bumps) |
| `diagnostic` | Debug events emitted during contract execution |

Filter out diagnostic events in most production integrations:
```bash
GET /v1/events?event_type=contract
```

## Event Data Size Limits

- Maximum `event_data` JSON size: `MAX_EVENT_DATA_BYTES` (default 64 KiB)
- Events exceeding this limit are logged and skipped
- The `soroban_pulse_events_oversized_total` counter tracks skipped events

## Versioning

The event schema is tied to the Soroban protocol version. Breaking changes are announced in `CHANGELOG.md` and the `schema_version` column in the database tracks the protocol version in effect when each event was stored.
