-- Issue #617: Schema versioning for contract_schemas table
-- Adds a version counter that increments on each update, allowing clients to
-- detect stale cached schemas and replay validation failures by version.

ALTER TABLE contract_schemas
    ADD COLUMN IF NOT EXISTS version INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS description TEXT;

-- Bump version automatically on every UPDATE
CREATE OR REPLACE FUNCTION increment_schema_version()
RETURNS TRIGGER AS $$
BEGIN
    NEW.version := OLD.version + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_schema_version ON contract_schemas;
CREATE TRIGGER trg_schema_version
    BEFORE UPDATE ON contract_schemas
    FOR EACH ROW EXECUTE FUNCTION increment_schema_version();

-- Index for listing schemas by version (useful for sync/audit)
CREATE INDEX IF NOT EXISTS idx_contract_schemas_version ON contract_schemas(version);

-- schema_validation_metrics: aggregated pass/fail counters per contract
-- Updated by the application layer; provides a persistent audit trail
-- independent of the Prometheus in-process counters (which reset on restart).
CREATE TABLE IF NOT EXISTS schema_validation_metrics (
    contract_id  TEXT PRIMARY KEY REFERENCES contract_schemas(contract_id) ON DELETE CASCADE,
    pass_count   BIGINT NOT NULL DEFAULT 0,
    fail_count   BIGINT NOT NULL DEFAULT 0,
    last_checked TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE schema_validation_metrics IS
    'Persistent pass/fail counters for JSON Schema validation per contract (issue #617).';
