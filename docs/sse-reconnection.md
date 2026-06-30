# SSE Client Reconnection

Soroban Pulse supports seamless SSE client reconnection via the standard
`Last-Event-ID` header.  Clients that disconnect and reconnect will receive all
events they missed automatically — no application-level polling or manual cursor
management is needed.

## How it works

1. **Every SSE event carries an `id:` field** — a UUID that uniquely identifies
   the event in the server's ring buffer.

2. **The browser (or any SSE-compliant client) stores the last received event
   ID** and sends it back as the `Last-Event-ID` HTTP header when it reconnects.

3. **The server replays missed events** from its in-memory ring buffer (capacity
   10 000 events, FIFO eviction) and then transitions to the live stream.

4. **If the event was evicted** (the client was disconnected longer than the
   buffer can cover), the server falls back to a database query to replay up to
   `SSE_REPLAY_LIMIT` events (default 1 000).

5. A **`replay_complete` event** is emitted after all replayed events and before
   the live stream begins, so clients can distinguish replayed from new events.

## Browser / JavaScript example

```js
const es = new EventSource('/v1/events/stream?contract_id=CABC...');

es.onmessage = (e) => {
  // The browser automatically tracks lastEventId and sends it on reconnect.
  console.log('event:', JSON.parse(e.data));
};

es.addEventListener('replay_complete', () => {
  console.log('caught up — now receiving live events');
});

es.addEventListener('lag', (e) => {
  // Emitted when the server-side broadcast channel lagged.
  const { missed } = JSON.parse(e.data);
  console.warn(`Missed ${missed} events in broadcast channel`);
});
```

The browser handles reconnection automatically.  The `Last-Event-ID` header is
set by the browser using the most recently received `id:` field.

## Custom client (Node / Python / Go)

```js
// Node.js using the 'eventsource' package
import EventSource from 'eventsource';

let lastEventId = null;

function connect() {
  const url = 'http://localhost:3000/v1/events/stream';
  const headers = lastEventId ? { 'Last-Event-ID': lastEventId } : {};
  const es = new EventSource(url, { headers });

  es.onmessage = (e) => {
    lastEventId = e.lastEventId;   // track it yourself when not using a browser
    handleEvent(JSON.parse(e.data));
  };

  es.onerror = () => {
    es.close();
    setTimeout(connect, 3000);    // reconnect after 3 s
  };
}

connect();
```

## Configuration

| Environment variable        | Default | Description                                             |
|-----------------------------|---------|----------------------------------------------------------|
| `SSE_RING_BUFFER_CAPACITY`  | 10000   | Max events held in the in-memory replay buffer           |
| `SSE_REPLAY_LIMIT`          | 1000    | Max events replayed per reconnection (DB fallback cap)   |
| `SSE_KEEPALIVE_INTERVAL_MS` | 15000   | How often a `ping` event is sent to keep the connection  |
| `SSE_MAX_LAG_BEFORE_DISCONNECT` | 0 (off) | Disconnect a client that has lagged this many events |

## Metrics

| Metric                                      | Description                                  |
|---------------------------------------------|----------------------------------------------|
| `soroban_pulse_sse_replayed_events_total`   | Total events replayed across all reconnects  |
| `soroban_pulse_sse_ring_buffer_size`        | Current number of events in the ring buffer  |
| `soroban_pulse_sse_ring_buffer_overflows_total` | Times the buffer evicted an old event    |
| `soroban_pulse_sse_ring_buffer_misses_total` | Replays that fell back to the database      |
| `soroban_pulse_sse_lagged_events_total`     | Events missed by a slow consumer (per conn) |

## Query result cache

Endpoints backed by materialized views (e.g. `/v1/contracts/{id}/event-counts`)
cache their results for `QUERY_CACHE_TTL_SECS` seconds (default 300 s, clamped
to 5–60 min).  Set the env var to control the trade-off between freshness and
database load:

```
QUERY_CACHE_TTL_SECS=600   # 10 min — lower DB load
QUERY_CACHE_MAX_CAPACITY=2000  # cache up to 2 000 distinct queries
```

Materialized views are refreshed on the `STATS_REFRESH_INTERVAL_SECS` schedule
(default 3 600 s / 1 h).  Each refresh cycle also:

* Emits **staleness metrics** (`soroban_pulse_matview_staleness_seconds`) per
  view, so you can alert when a view has not been refreshed within the expected
  window.
* Runs **EXPLAIN** on representative queries and records estimated row counts
  (`soroban_pulse_query_plan_estimated_rows`) for capacity-planning dashboards.
