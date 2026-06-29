CREATE TABLE IF NOT EXISTS webhook_retry_queue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    url TEXT NOT NULL,
    payload JSONB NOT NULL,
    secret_hash TEXT,
    attempt INT NOT NULL DEFAULT 0,
    max_attempts INT NOT NULL DEFAULT 5,
    next_retry_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_error TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'processing', 'succeeded', 'failed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_webhook_retry_queue_status_next_retry
    ON webhook_retry_queue (status, next_retry_at)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_webhook_retry_queue_created_at
    ON webhook_retry_queue (created_at DESC);
