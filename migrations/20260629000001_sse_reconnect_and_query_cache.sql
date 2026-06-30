-- Migration: SSE reconnection ring buffer + materialized view for contract event counts
-- Issue: feat/sse-reconnect-and-query-cache

-- Materialized view: per-contract event counts (daily rollup).
-- Used by the query result cache to serve contract-level analytics without
-- hitting the raw events table on every request.
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_contract_event_counts AS
SELECT
    contract_id,
    event_type,
    DATE_TRUNC('day', timestamp) AS event_day,
    COUNT(*)                     AS event_count,
    COUNT(DISTINCT tx_hash)      AS unique_tx_count,
    MAX(timestamp)               AS last_event_at
FROM events
GROUP BY contract_id, event_type, DATE_TRUNC('day', timestamp)
WITH DATA;

-- Unique index required for REFRESH MATERIALIZED VIEW CONCURRENTLY.
CREATE UNIQUE INDEX IF NOT EXISTS mv_contract_event_counts_uniq
    ON mv_contract_event_counts (contract_id, event_type, event_day);

-- Index to support quick lookups by contract.
CREATE INDEX IF NOT EXISTS mv_contract_event_counts_contract_idx
    ON mv_contract_event_counts (contract_id);

-- Materialized view: daily event summaries (if not already present).
CREATE MATERIALIZED VIEW IF NOT EXISTS events_daily_summary AS
SELECT
    DATE_TRUNC('day', timestamp) AS event_day,
    COUNT(*)                     AS event_count,
    COUNT(DISTINCT contract_id)  AS unique_contracts,
    COUNT(DISTINCT tx_hash)      AS unique_txs
FROM events
GROUP BY DATE_TRUNC('day', timestamp)
WITH DATA;

CREATE UNIQUE INDEX IF NOT EXISTS events_daily_summary_uniq
    ON events_daily_summary (event_day);
