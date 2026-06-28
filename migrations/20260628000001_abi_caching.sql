-- Issue #607: Add TTL and fetch tracking to contract_abis for cache management.
ALTER TABLE contract_abis
    ADD COLUMN IF NOT EXISTS expires_at  TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS fetched_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS abi_hash    TEXT,
    ADD COLUMN IF NOT EXISTS is_valid    BOOLEAN NOT NULL DEFAULT true;

-- Index for TTL-based expiry sweeps.
CREATE INDEX IF NOT EXISTS idx_contract_abis_expires_at
    ON contract_abis(expires_at)
    WHERE expires_at IS NOT NULL;

-- ABI validation log for schema mismatch auditing.
CREATE TABLE IF NOT EXISTS contract_abi_validation_log (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    contract_id TEXT NOT NULL,
    error       TEXT NOT NULL,
    logged_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_abi_validation_log_contract
    ON contract_abi_validation_log(contract_id);
