DROP TABLE IF EXISTS contract_abi_validation_log;
DROP INDEX IF EXISTS idx_contract_abis_expires_at;
ALTER TABLE contract_abis
    DROP COLUMN IF EXISTS expires_at,
    DROP COLUMN IF EXISTS fetched_at,
    DROP COLUMN IF EXISTS abi_hash,
    DROP COLUMN IF EXISTS is_valid;
