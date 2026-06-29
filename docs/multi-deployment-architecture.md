# Multi-Deployment Architecture Guide

Guidance for running SorobanPulse across multiple regions or cloud providers to achieve high availability and geo-redundancy.

## Overview

A multi-deployment setup runs two or more SorobanPulse instances in separate failure domains (regions, availability zones, or cloud providers). Each instance maintains its own database replica, but event indexing is coordinated via an advisory lock so only one instance writes new events at a time.

```
┌──────────────────────┐      ┌──────────────────────┐
│   Region A (Primary) │      │  Region B (Standby)  │
│                      │      │                      │
│  ┌────────────────┐  │      │  ┌────────────────┐  │
│  │  SorobanPulse  │  │      │  │  SorobanPulse  │  │
│  │  (Indexer +    │  │      │  │  (HTTP only    │  │
│  │   HTTP)        │  │      │  │   + standby)   │  │
│  └───────┬────────┘  │      │  └───────┬────────┘  │
│          │           │      │          │           │
│  ┌───────▼────────┐  │ Repl │  ┌───────▼────────┐  │
│  │   PostgreSQL   │◄─┼──────┼──│   PostgreSQL   │  │
│  │   (Primary)    │  │      │  │   (Replica)    │  │
│  └────────────────┘  │      │  └────────────────┘  │
└──────────────────────┘      └──────────────────────┘
         ▲                              ▲
         └──────────── DNS / LB ────────┘
```

## Geo-Redundancy Patterns

### Pattern 1: Active-Passive with Streaming Replication

The simplest production setup. One region is active (holds the advisory lock and indexes events); the other is passive (read-only HTTP, ready to promote).

**Setup:**
1. Configure PostgreSQL streaming replication from Region A → Region B.
2. Deploy SorobanPulse in both regions with identical configuration.
3. Both instances connect to their local database. Region B connects to the replica in read-only mode.
4. Region A wins the advisory lock on startup; Region B serves HTTP from the replica.

**Failover:** When Region A becomes unavailable, promote the Region B replica and restart its SorobanPulse instance. It will acquire the advisory lock and begin indexing.

### Pattern 2: Active-Active with Read Distribution

Both regions serve HTTP traffic. Indexing remains in one region (lock holder), but read queries are distributed across both replicas via a load balancer.

**Setup:**
1. Configure streaming replication as above.
2. Deploy SorobanPulse in both regions.
3. Place a global load balancer (AWS Route 53, Cloudflare, GCP GLB) in front of both regions.
4. Use health checks to route write-path traffic only to the primary region.

**Trade-offs:**
- Reads are distributed, improving throughput.
- Replication lag can cause stale reads on the replica. Use `PGRST_DB_USE_LEGACY_GUCS=false` and `SET TRANSACTION READ ONLY` if consistency is critical.

### Pattern 3: Multi-Cloud Active-Passive

Same as Pattern 1 but across different cloud providers (e.g., AWS + GCP) for maximum blast-radius isolation.

**Considerations:**
- Cross-cloud replication incurs egress costs and higher latency (~20–50 ms typical).
- Use a VPN or dedicated interconnect (AWS Direct Connect / GCP Interconnect) for the replication channel.
- Certificate pinning and mutual TLS between the replication endpoints is strongly recommended.

## Failover Documentation

### Automated Failover (Patroni / pg_auto_failover)

For production deployments, manage promotion automatically with a tool like [Patroni](https://patroni.readthedocs.io/) or [pg_auto_failover](https://pg-auto-failover.readthedocs.io/).

```yaml
# Example Patroni config snippet
bootstrap:
  dcs:
    ttl: 30
    loop_wait: 10
    retry_timeout: 10
    maximum_lag_on_failover: 1048576  # 1 MB
```

When Patroni promotes the replica, SorobanPulse's database connection pool will see a connection error, reconnect, and re-attempt the advisory lock. The first instance to reconnect to the new primary will acquire the lock and resume indexing.

### Manual Failover Procedure

1. **Confirm primary is down:**
   ```bash
   psql $DATABASE_URL_REGION_A -c "SELECT 1"
   ```

2. **Promote the replica:**
   ```bash
   # On the Region B PostgreSQL host
   pg_ctl promote -D /var/lib/postgresql/data
   # Or for managed databases:
   aws rds failover-db-cluster --db-cluster-identifier soroban-pulse
   ```

3. **Update `DATABASE_URL` in Region B** to point to the now-promoted instance.

4. **Restart the Region B SorobanPulse instance.** It will acquire the advisory lock and start indexing from the last checkpoint stored in `indexer_checkpoints`.

5. **Verify recovery:**
   ```bash
   curl https://region-b.pulse.example.com/healthz/ready
   curl https://region-b.pulse.example.com/v1/metrics | grep soroban_pulse_indexer_lag
   ```

6. **Update DNS** (if not managed automatically) to point traffic to Region B.

### Recovery Time Objectives

| Scenario | RTO (manual) | RTO (automated) |
|---|---|---|
| App process crash | < 30 s (restart) | < 10 s (Kubernetes) |
| Single AZ outage | 5–10 min | 1–2 min (Patroni) |
| Full region outage | 10–20 min | 2–5 min |
| Cloud provider outage | 20–60 min | 10–15 min |

## Cross-Region Sync

### Database Replication

SorobanPulse relies on standard PostgreSQL logical or physical replication. Physical (streaming) replication is recommended for most deployments:

```sql
-- On the primary, create a replication slot
SELECT pg_create_physical_replication_slot('region_b_slot');

-- On the replica, set recovery.conf / postgresql.auto.conf
primary_conninfo = 'host=db-primary.region-a.internal port=5432 user=replicator password=...'
primary_slot_name = 'region_b_slot'
```

**Replication lag monitoring:** The `soroban_pulse_indexer_lag` metric tracks how far behind the indexer is from the chain tip. A separate lag metric from the replica itself (`pg_wal_lsn_diff`) should be monitored via the Prometheus job scraping the replica's `pg_stat_replication`.

### Configuration Sync

Sync these configuration items across regions:

| Item | Sync method |
|---|---|
| `API_KEY` / `ADMIN_API_KEY` | Secret manager (AWS Secrets Manager, GCP Secret Manager) |
| `WEBHOOK_SECRET` | Secret manager |
| `EVENT_DATA_ENCRYPTION_KEY` | Secret manager — **must be identical across regions** |
| `config.toml` | Git (deploy via CI/CD) |
| Notification channel config | Stored in the database; replicated automatically |

### Subscription and Webhook Consistency

Subscriptions and webhook channel registrations are stored in the PostgreSQL database and replicated to all standby nodes. When failing over:

- Active SSE connections to the failed region will drop and clients will reconnect (standard EventSource retry).
- Webhook deliveries in-flight at the time of failure will be retried from the `webhook_retry_queue` table, which is replicated and becomes writable on the new primary.
- No manual intervention is required for subscriptions.

## Kubernetes Multi-Region Deployment

```yaml
# Region A deployment (indexer + HTTP)
apiVersion: apps/v1
kind: Deployment
metadata:
  name: soroban-pulse
  namespace: soroban-pulse
spec:
  replicas: 2
  template:
    spec:
      containers:
      - name: soroban-pulse
        image: soroban-pulse:latest
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: soroban-pulse-secrets
              key: database-url-region-a
        - name: STELLAR_RPC_URL
          value: "https://soroban-mainnet.stellar.org"
```

```yaml
# Region B deployment (HTTP-only standby)
# Same spec but DATABASE_URL points to the read replica.
# When Region A fails, update DATABASE_URL to the promoted primary and scale replicas.
```

## Health Checks and Observability

Both regions should scrape the same Prometheus metrics. Use an alert to detect replication lag exceeding a threshold:

```yaml
# docs/alerts.yml addition
- alert: ReplicationLagHigh
  expr: pg_replication_lag_seconds > 30
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "PostgreSQL replication lag is {{ $value }}s"
```

Monitor cross-region readiness with:
```bash
# Check both regions are healthy
curl https://region-a.pulse.example.com/healthz/ready
curl https://region-b.pulse.example.com/healthz/ready

# Confirm only one region is actively indexing
curl https://region-a.pulse.example.com/v1/metrics | grep soroban_pulse_indexer_is_leader
curl https://region-b.pulse.example.com/v1/metrics | grep soroban_pulse_indexer_is_leader
```
