-- Issue #608: Dedicated ledger_hashes table for proof-of-indexing and hash chain verification.
CREATE TABLE IF NOT EXISTS ledger_hashes (
    ledger      BIGINT      PRIMARY KEY,
    hash        TEXT        NOT NULL,
    prev_hash   TEXT,
    indexed_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for range look-ups in the hash-chain verifier.
CREATE INDEX IF NOT EXISTS idx_ledger_hashes_indexed_at ON ledger_hashes(indexed_at);
