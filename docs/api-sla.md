# API Response Time SLA

This document defines SorobanPulse's latency targets, SLA guarantees, measurement methodology, and the monitoring dashboard used to track compliance.

## Latency Targets

The following targets apply to the **SorobanPulse managed deployment**. Self-hosted deployments should adjust targets to reflect their own infrastructure.

### REST API Endpoints

| Endpoint | p50 | p95 | p99 |
|---|---|---|---|
| `GET /v1/events` (paginated) | ≤ 25 ms | ≤ 100 ms | ≤ 250 ms |
| `GET /v1/events/:id` | ≤ 10 ms | ≤ 40 ms | ≤ 100 ms |
| `POST /v1/subscriptions` | ≤ 20 ms | ≤ 80 ms | ≤ 200 ms |
| `GET /v1/subscriptions` | ≤ 15 ms | ≤ 60 ms | ≤ 150 ms |
| `POST /v1/webhooks` | ≤ 20 ms | ≤ 80 ms | ≤ 200 ms |
| `GET /health` | ≤ 5 ms | ≤ 20 ms | ≤ 50 ms |
| `GET /metrics` | ≤ 10 ms | ≤ 40 ms | ≤ 100 ms |

Latency is measured **at the server** from the moment the first byte of the request is received to the moment the last byte of the response is written. Network transit time between the client and the server is not included.

### SSE Streaming

| Metric | Target |
|---|---|
| Time to first event (cold connection) | ≤ 500 ms |
| Event propagation latency (ledger close → SSE delivery) | ≤ 2 s (p95) |
| Keepalive ping interval | Configurable; default 15 s |

### Webhook Delivery

| Metric | Target |
|---|---|
| Time from event indexed → first delivery attempt | ≤ 5 s (p95) |
| Delivery throughput per webhook | Up to 100 deliveries/min |
| Maximum retry window (all attempts) | 24 h |

### Indexer

| Metric | Target |
|---|---|
| Ledger processing lag (RPC → indexed) | ≤ 10 s (p95) in normal conditions |
| Lag warning threshold | `INDEXER_LAG_WARN_THRESHOLD` ledgers (default: 100) |

## SLA Guarantees

The following monthly SLA commitments apply to the managed SorobanPulse service.

| Tier | Monthly uptime | Max downtime / month |
|---|---|---|
| Standard | 99.5% | ~3.6 h |
| Professional | 99.9% | ~43 min |
| Enterprise | 99.95% | ~21 min |

**Uptime** is defined as the percentage of minutes in a calendar month during which the `/health` endpoint returns `HTTP 200` with a valid database connection.

**Downtime** is defined as any period longer than 1 consecutive minute during which `/health` returns a non-`200` status or is unreachable.

Planned maintenance windows are excluded from the downtime calculation if announced at least 24 hours in advance via the status page.

### SLA credits

| Uptime achieved | Service credit |
|---|---|
| 99.0% – 99.5% (Standard) | 10% of monthly fee |
| 95.0% – 99.0% | 25% of monthly fee |
| < 95.0% | 50% of monthly fee |

Credits are applied to the next billing cycle and do not accrue as cash refunds.

## SLA Calculation Methodology

### Uptime percentage

```
uptime_pct = (total_minutes - downtime_minutes) / total_minutes × 100
```

For a 30-day month: `total_minutes = 43 200`.

Downtime minutes are summed from the status-page incident log, rounding fractional minutes up.

### Latency percentile calculation

Latency percentiles are computed over a **rolling 5-minute window** of completed requests, using a t-digest algorithm (Prometheus `histogram_quantile`). The Prometheus metric is:

```
http_request_duration_seconds{handler="/v1/events", quantile="0.95"}
```

The p99 breach threshold is evaluated per 15-minute evaluation window. A single spike within 15 minutes that does not persist does not constitute an SLA breach.

### Indexer lag calculation

```
indexer_lag_ledgers = current_ledger_on_rpc - latest_indexed_ledger
```

Lag is sampled every 30 seconds. An alert fires when `indexer_lag_ledgers > INDEXER_LAG_WARN_THRESHOLD` for more than 5 consecutive samples (i.e., 2.5 minutes of sustained lag).

## SLA Monitoring Dashboard

### Prometheus metrics exposed by SorobanPulse

| Metric name | Type | Description |
|---|---|---|
| `http_request_duration_seconds` | Histogram | Per-handler request latency with `handler`, `method`, `status` labels |
| `http_requests_total` | Counter | Total requests, labelled by `handler`, `method`, `status` |
| `indexer_lag_ledgers` | Gauge | Number of ledgers the indexer is behind the RPC head |
| `indexer_events_processed_total` | Counter | Cumulative events indexed |
| `webhook_delivery_duration_seconds` | Histogram | Time from delivery attempt start to response |
| `webhook_deliveries_total` | Counter | Deliveries labelled by `status` (`success`, `failed`, `retrying`) |
| `sse_active_connections` | Gauge | Current number of open SSE connections |
| `db_query_duration_seconds` | Histogram | Database query latency with `query` label |
| `slow_queries_total` | Counter | Queries exceeding `SLOW_QUERY_THRESHOLD_MS` |

### Grafana dashboard

The pre-built Grafana dashboard ([`docs/grafana-dashboard.json`](grafana-dashboard.json)) includes the following panels:

| Panel | Query | Alert threshold |
|---|---|---|
| REST p50 / p95 / p99 latency | `histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m]))` | p99 > 500 ms |
| Request rate (RPS) | `rate(http_requests_total[1m])` | — |
| Error rate | `rate(http_requests_total{status=~"5.."}[5m]) / rate(http_requests_total[5m])` | > 1% |
| Indexer lag | `indexer_lag_ledgers` | > 100 ledgers |
| Webhook delivery success rate | `rate(webhook_deliveries_total{status="success"}[5m]) / rate(webhook_deliveries_total[5m])` | < 95% |
| Active SSE connections | `sse_active_connections` | — |
| Slow queries | `rate(slow_queries_total[5m])` | > 0.1/s |
| DB query p95 | `histogram_quantile(0.95, rate(db_query_duration_seconds_bucket[5m]))` | > 1 s |

Import the dashboard into Grafana:

```bash
curl -X POST http://localhost:3000/api/dashboards/import \
  -H "Content-Type: application/json" \
  -d @docs/grafana-dashboard.json
```

### Alerting rules

The alert configuration at [`docs/alerts.yml`](alerts.yml) defines Prometheus alerting rules. Key alerts relevant to SLA monitoring:

```yaml
# High p99 latency
- alert: HighAPILatencyP99
  expr: histogram_quantile(0.99, rate(http_request_duration_seconds_bucket[5m])) > 0.5
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "p99 API latency exceeds 500 ms"

# Indexer lag
- alert: IndexerLagHigh
  expr: indexer_lag_ledgers > 100
  for: 3m
  labels:
    severity: critical
  annotations:
    summary: "Indexer is more than 100 ledgers behind"

# Webhook failure rate
- alert: WebhookDeliveryFailureHigh
  expr: rate(webhook_deliveries_total{status="failed"}[5m]) > 0.05
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "Webhook failure rate exceeds 5%"
```

## SLA Exceptions

The following conditions are **excluded** from SLA calculations and do not count as downtime:

### Planned maintenance

Maintenance windows announced ≥ 24 hours in advance on the status page are excluded. Maintenance is typically scheduled between 02:00–04:00 UTC on weekdays.

### Force majeure

Outages caused by events outside reasonable control are excluded, including:
- Upstream Stellar network forks, resets, or RPC outages
- Major cloud provider incidents affecting entire availability zones
- Internet routing failures outside the operator's network

### Client-side issues

Latency attributable to the following is excluded:
- Client network conditions
- DNS resolution time
- TLS handshake for fresh connections
- Large response payloads requested by the client (e.g., `limit=10000` with wide filter)

### Beta and preview features

Endpoints or features explicitly documented as **beta** or **preview** carry no latency guarantees and are excluded from all SLA calculations.

### Rate-limited requests

Requests that return `429 Too Many Requests` are not counted against the SLA error budget because they are a defined, expected response for clients exceeding their rate limit.

### Self-hosted deployments

SLA guarantees apply only to the **managed SorobanPulse service**. Operators running self-hosted deployments are responsible for defining and monitoring their own SLAs.

## Reporting and Incident Response

- **Status page**: Real-time uptime and incident history are published at `https://status.sorobanpulse.io` (managed deployment).
- **Incident response**: P1 (full outage) SLA target is acknowledgement within 15 minutes and mitigation within 1 hour.
- **Post-mortems**: All P1 and P2 incidents receive a public post-mortem within 5 business days.
- **Monthly SLA report**: Available in the operator dashboard under **Settings → SLA Reports**.

## Related Documentation

- [Grafana dashboard](grafana-dashboard.json)
- [Alert rules](alerts.yml)
- [Capacity planning](capacity-planning.md)
- [Performance regression testing](performance-regression-testing.md)
- [Replica sync monitoring](replica-monitoring.md)
