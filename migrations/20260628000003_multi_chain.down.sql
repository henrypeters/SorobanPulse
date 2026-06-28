DROP TABLE IF EXISTS networks;
DROP INDEX IF EXISTS idx_events_chain_contract;
DROP INDEX IF EXISTS idx_events_chain_id;
ALTER TABLE events DROP COLUMN IF EXISTS chain_id;
