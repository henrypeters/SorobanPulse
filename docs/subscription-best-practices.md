# Subscription Best Practices

This guide covers how to configure subscriptions for maximum reliability and efficiency in SorobanPulse.

## Filter Optimization

Overly broad filters generate unnecessary traffic and waste downstream processing. Narrow filters at the source.

### Use specific contract IDs

Always scope subscriptions to a contract address. Wildcard subscriptions that match every event on the network should be avoided unless you genuinely need full-chain coverage.

```json
// Good — scoped to one contract
{
  "contract_id": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
  "topics": ["transfer"]
}

// Avoid — matches every event across all contracts
{
  "contract_id": "*"
}
```

### Filter by topic at subscription time, not application time

The indexer evaluates `topics` filters server-side before any delivery is attempted. Pushing filtering to your consumer wastes bandwidth and delivery quota.

```json
// Good — server-side topic filter
{
  "contract_id": "CDLZFC3...",
  "topics": ["transfer", "mint"]
}

// Avoid — subscribing to everything and filtering in your webhook handler
{
  "contract_id": "CDLZFC3..."
}
```

### Combine filters with AND semantics

When specifying multiple criteria they are evaluated as AND conditions, so the narrower you make each clause, the fewer events get delivered:

| Filter field | Description | Tip |
|---|---|---|
| `contract_id` | Exact contract address | Always provide |
| `topics` | Event topic names | Provide even if you want all topics for one contract |
| `value_gt` / `value_lt` | Numeric threshold on the event value field | Useful for alert-style subscriptions |
| `network` | `testnet` or `mainnet` | Always set explicitly to avoid cross-network leakage |

### Avoid redundant subscriptions

Each additional subscription adds a database row checked on every indexed event. Audit your subscriptions periodically:

```bash
# List all subscriptions for your API key
curl -H "X-API-Key: $API_KEY" "$BASE_URL/v1/subscriptions" | jq '.[].filters'
```

Merge subscriptions that share the same contract and webhook endpoint into one subscription with a broader `topics` list rather than maintaining separate subscriptions per topic.

## Delivery Frequency Recommendations

### Real-time streaming (SSE)

Use SSE when you need sub-second latency and your consumer can hold a persistent HTTP connection:

```javascript
const es = new EventSource(`${BASE_URL}/v1/events/stream?contract_id=CDLZFC3...`, {
  headers: { "X-API-Key": API_KEY }
});

es.onmessage = (e) => {
  const event = JSON.parse(e.data);
  processEvent(event);
};
```

SSE connections are long-lived. Configure your load balancer or reverse proxy to allow at least 60 seconds of idle timeout (or use `keepalive` pings — set `SSE_KEEPALIVE_SECS` in your server config).

### Webhook delivery

Webhooks are the right choice when you need durable, at-least-once delivery with retry guarantees and your consumer does not need to maintain a persistent connection.

Sizing guidance for webhook endpoints:

| Event rate | Recommended endpoint setup |
|---|---|
| < 10 events/min | Single handler; synchronous processing is fine |
| 10–100 events/min | Enqueue to an internal queue (Redis/SQS) and process async |
| > 100 events/min | Autoscaling consumer group; return `200` immediately and process in background |

**Always return `200 OK` as fast as possible.** SorobanPulse marks a delivery as failed if your endpoint takes longer than `WEBHOOK_TIMEOUT_MS` (default 10 seconds) to respond, and schedules a retry regardless of what your handler ultimately does.

### Batch processing via REST

If you do not need real-time delivery, poll the REST API on a schedule and process events in bulk. This is the most resilient pattern for batch analytics workloads:

```bash
# Fetch all events since last checkpoint
curl "$BASE_URL/v1/events?contract_id=CDLZFC3...&after_ledger=$LAST_LEDGER&limit=500"
```

Store `$LAST_LEDGER` in your application state after each successful batch so you can resume from the correct position after a restart.

## Backoff Strategies

### Webhook retry schedule (server-side)

SorobanPulse retries failed webhook deliveries using exponential backoff with jitter. The default schedule:

| Attempt | Delay |
|---|---|
| 1 | Immediate |
| 2 | 30 s ± 5 s jitter |
| 3 | 2 min ± 15 s jitter |
| 4 | 10 min ± 1 min jitter |
| 5 | 1 h ± 5 min jitter |

After 5 failed attempts the delivery is marked `failed` and no further retries are made. A `delivery_failed` notification is emitted and logged in `delivery_logs`.

Configure the retry count with `WEBHOOK_MAX_RETRIES` (default `5`). Setting this to `0` disables retries.

### Client-side SSE reconnection

When an SSE connection drops, reconnect with exponential backoff to avoid thundering-herd behaviour at the server:

```javascript
function connectSSE(url, apiKey, lastEventId) {
  let delay = 1000;
  const MAX_DELAY = 30_000;

  function connect() {
    const headers = { "X-API-Key": apiKey };
    if (lastEventId) headers["Last-Event-ID"] = lastEventId;

    const es = new EventSource(url, { headers });

    es.onmessage = (e) => {
      delay = 1000;           // reset on success
      lastEventId = e.lastEventId;
      processEvent(JSON.parse(e.data));
    };

    es.onerror = () => {
      es.close();
      setTimeout(() => {
        delay = Math.min(delay * 2, MAX_DELAY);
        connect();
      }, delay + Math.random() * 500);
    };
  }

  connect();
}
```

Pass the `Last-Event-ID` header on reconnect — SorobanPulse resumes the stream from that event so no events are lost during the gap.

### Client-side REST polling backoff

If the REST API returns `429 Too Many Requests` or `503 Service Unavailable`, respect the `Retry-After` header:

```python
import time, requests

def fetch_events(url, api_key, last_ledger):
    delay = 1
    while True:
        r = requests.get(url, headers={"X-API-Key": api_key},
                         params={"after_ledger": last_ledger, "limit": 500})
        if r.status_code == 200:
            return r.json()
        elif r.status_code in (429, 503):
            retry_after = int(r.headers.get("Retry-After", delay))
            time.sleep(retry_after)
            delay = min(delay * 2, 60)
        else:
            r.raise_for_status()
```

## Common Patterns

### Alert on threshold-crossing events

Subscribe to a contract and use `value_gt` to receive only events where the transferred amount exceeds a threshold:

```json
{
  "contract_id": "CDLZFC3...",
  "topics": ["transfer"],
  "filters": {
    "value_gt": 1000000
  },
  "webhook_url": "https://alerts.example.com/large-transfer",
  "channels": ["email", "pagerduty"]
}
```

### Fan-out to multiple destinations

A single subscription can deliver to multiple channels simultaneously. Prefer one subscription over multiple identical ones:

```json
{
  "contract_id": "CDLZFC3...",
  "topics": ["transfer", "mint", "burn"],
  "webhook_url": "https://api.example.com/events",
  "channels": ["email", "slack", "kinesis"]
}
```

### Resume-safe cursor pattern

Track the highest processed ledger sequence in your application and always pass it as `after_ledger`. This makes your consumer idempotent:

```rust
let mut cursor = load_cursor_from_db().await?;

loop {
    let events = client.get_events(contract_id, cursor).await?;
    for event in &events {
        process(event).await?;
        cursor = event.ledger_sequence;
    }
    save_cursor_to_db(cursor).await?;
    tokio::time::sleep(Duration::from_secs(5)).await;
}
```

### Testing subscriptions in development

Use the testnet and a local tunnel (e.g. [ngrok](https://ngrok.com/)) to receive webhooks on your local machine during development:

```bash
# Start local tunnel
ngrok http 8080

# Register webhook pointing at tunnel URL
curl -X POST "$BASE_URL/v1/webhooks" \
  -H "X-API-Key: $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"url": "https://<ngrok-id>.ngrok.io/webhook", "secret": "dev-secret"}'
```

Verify HMAC signatures even in development to ensure your verification logic works before going to production. See [webhook-verification.md](webhook-verification.md) for the signature algorithm.

### Dead-letter handling

Monitor `delivery_logs` for `status = 'failed'` rows and set up an alert when the failure count rises unexpectedly:

```sql
SELECT webhook_id, COUNT(*) AS failures
FROM delivery_logs
WHERE status = 'failed'
  AND created_at > NOW() - INTERVAL '1 hour'
GROUP BY webhook_id
HAVING COUNT(*) > 10;
```

When a webhook endpoint is persistently unavailable, consider temporarily disabling it via `PATCH /v1/webhooks/:id` and re-enabling it once the endpoint recovers. This prevents your retry queue from building up.

## Related Documentation

- [Webhook verification](webhook-verification.md)
- [Notification rate limiting](notification-rate-limiting.md)
- [Notification delivery receipts](notification-delivery-receipts.md)
- [Code generation helpers for subscriptions](codegen.md)
