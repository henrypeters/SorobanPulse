-- Issue #624: Batch download tracking table
CREATE TABLE IF NOT EXISTS batch_download_jobs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    status      TEXT NOT NULL DEFAULT 'pending',
    format      TEXT NOT NULL DEFAULT 'json',
    filter_count INT NOT NULL DEFAULT 0,
    event_count  BIGINT NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    error       TEXT
);

CREATE INDEX IF NOT EXISTS idx_batch_jobs_status ON batch_download_jobs (status);
CREATE INDEX IF NOT EXISTS idx_batch_jobs_created_at ON batch_download_jobs (created_at DESC);
