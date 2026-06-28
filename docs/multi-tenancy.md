# Multi-Tenancy Deployment

SorobanPulse supports isolating event data between multiple tenants in a single deployment (issue #583). Each tenant sees only their own events; queries, streams, and exports are automatically scoped by the resolved tenant identity.

## Architecture

```
Client (API key A) ──► auth middleware ──► resolves tenant-a ──► events WHERE tenant_id = 'tenant-a'
Client (API key B) ──► auth middleware ──► resolves tenant-b ──► events WHERE tenant_id = 'tenant-b'
Admin  (admin key) ──► auth middleware ──► no tenant scope  ──► all events
```

Tenant identity is derived from the API key: the SHA-256 hash of the raw API key is looked up in the `api_key_tenants` table, which maps key hashes to tenant identifiers. The plaintext key is never stored in the database.

## Schema

The `events` table gains a `tenant_id TEXT` column (migration `20260430000000_add_tenant_id.sql`). A `NULL` value means the row belongs to the default single-tenant deployment.

The `api_key_tenants` table maps key hashes to tenants:

```sql
CREATE TABLE api_key_tenants (
    key_hash  TEXT PRIMARY KEY,   -- SHA-256(raw_api_key) hex
    tenant_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

## Configuration

### Environment variables

| Variable | Default | Description |
|---|---|---|
| `MULTI_TENANT` | `false` | Set to `true` to enable multi-tenant mode |
| `TENANT_RATE_LIMIT_PER_MINUTE` | `RATE_LIMIT_PER_MINUTE` | Independent per-tenant request quota |
| `INDEXER_TENANT_ID` | — | Tenant stamped on events by this indexer instance |
| `TENANT_CONTRACT_FILTER` | — | Per-tenant contract allowlist for the indexer |

### Enable multi-tenant mode

```bash
MULTI_TENANT=true
```

When enabled:
- Every non-admin API key must have a row in `api_key_tenants`.
- A key with no tenant mapping returns `403 Forbidden`.
- All event queries are automatically filtered to the resolved `tenant_id`.
- The indexer stamps every inserted event with `INDEXER_TENANT_ID`.

### Register a tenant API key

Insert the SHA-256 hash of the raw key (SorobanPulse uses the same hashing function):

```bash
KEY="your-tenant-api-key"
KEY_HASH=$(echo -n "$KEY" | sha256sum | awk '{print $1}')

psql "$DATABASE_URL" -c "
  INSERT INTO api_key_tenants (key_hash, tenant_id)
  VALUES ('$KEY_HASH', 'tenant-a')
  ON CONFLICT (key_hash) DO UPDATE SET tenant_id = EXCLUDED.tenant_id;
"
```

After inserting, restart the service so the in-memory tenant map is reloaded (or set `MULTI_TENANT=true` before the first start to load from the database at startup).

## Tenant routing middleware

The `auth_middleware` in `src/middleware.rs` handles tenant resolution:

1. Extracts the raw API key from `Authorization: Bearer <key>` or `X-Api-Key`.
2. Computes `SHA-256(key)`.
3. Looks up the hash in the in-memory `tenant_map` (loaded from `api_key_tenants` at startup).
4. Injects a `TenantId` extension into the request for downstream handlers.

Admin keys (configured via `ADMIN_API_KEY`) bypass tenant resolution and can access all tenant data.

## Tenant isolation validation

All event query handlers read the `TenantId` extension and append a `tenant_id = $N` condition to every SQL query. This is enforced in:

- `GET /v1/events`
- `GET /v1/events/contract/:id`
- `GET /v1/events/tx/:hash`
- `GET /v1/events/stream` (SSE)
- `GET /v1/events/stream/multi` (SSE)
- `GET /v1/events/export`
- `GET /v1/events/stats`

Events broadcast on SSE channels are additionally filtered in the streaming loop: events whose `tenant_id` does not match the subscriber's resolved tenant are dropped before delivery.

## Per-tenant rate limiting

When `MULTI_TENANT=true`, the HTTP rate limiter keys on the API key rather than the client IP address. This gives each tenant an independent quota so one tenant cannot exhaust the shared IP-based limit.

Set the per-tenant quota independently from the global IP rate limit:

```bash
MULTI_TENANT=true
TENANT_RATE_LIMIT_PER_MINUTE=120   # each tenant gets 120 req/min
RATE_LIMIT_PER_MINUTE=60           # fallback if TENANT_RATE_LIMIT_PER_MINUTE is unset
```

When `TENANT_RATE_LIMIT_PER_MINUTE` is unset, the value of `RATE_LIMIT_PER_MINUTE` is used for each tenant bucket.

## Per-tenant contract filtering (indexer)

Use `TENANT_CONTRACT_FILTER` to restrict which contracts the indexer stores per tenant:

```bash
TENANT_CONTRACT_FILTER=tenant-a:CABC...,CDEF...;tenant-b:CXYZ...
```

Events whose `contract_id` is not in the tenant's allowlist are dropped before storage. An empty allowlist means all contracts are indexed for that tenant.

## Multi-indexer deployment

Run one indexer instance per tenant, each writing to the same shared database:

```yaml
# indexer-tenant-a
env:
  - name: MULTI_TENANT
    value: "true"
  - name: INDEXER_TENANT_ID
    value: tenant-a
  - name: TENANT_CONTRACT_FILTER
    value: "tenant-a:CABC...,CDEF..."

# indexer-tenant-b
env:
  - name: MULTI_TENANT
    value: "true"
  - name: INDEXER_TENANT_ID
    value: tenant-b
  - name: TENANT_CONTRACT_FILTER
    value: "tenant-b:CXYZ..."
```

A single API tier serves all tenants; the indexers write to isolated rows in the shared `events` table.

## Kubernetes example

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: soroban-pulse-tenant-keys
stringData:
  tenant-a-key: "your-tenant-a-api-key"
  tenant-b-key: "your-tenant-b-api-key"
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: soroban-pulse-api
spec:
  template:
    spec:
      containers:
        - name: api
          env:
            - name: MULTI_TENANT
              value: "true"
            - name: TENANT_RATE_LIMIT_PER_MINUTE
              value: "120"
            - name: API_KEY
              valueFrom:
                secretKeyRef:
                  name: soroban-pulse-tenant-keys
                  key: tenant-a-key
```

## Security considerations

- API key hashes (SHA-256) are stored instead of plaintext keys. A compromised database does not expose raw keys.
- The `tenant_id` filter is applied unconditionally at the SQL layer; a missing or invalid `TenantId` extension results in no results returned (not an error) to prevent data leakage.
- Admin keys are the only path to cross-tenant data; protect them with `ADMIN_API_KEY` and restrict access to admin routes.
- Combine multi-tenancy with event encryption (`EVENT_DATA_ENCRYPTION_KEY`) for defence-in-depth: even if a tenant's `event_data` rows are accessed by another tenant via a misconfiguration, the payload remains encrypted.
