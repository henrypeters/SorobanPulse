CREATE TABLE IF NOT EXISTS indexer_bloom_state (
    id SERIAL PRIMARY KEY,
    capacity INT NOT NULL,
    fp_rate DOUBLE PRECISION NOT NULL,
    bitmap SMALLINT[] NOT NULL,
    persisted_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_bloom_state_persisted_at ON indexer_bloom_state(persisted_at DESC);
