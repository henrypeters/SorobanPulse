-- Issue #324: Add composite indexes for improved query performance
-- These indexes accelerate the most common filter combinations used by API consumers

-- Composite index for contract_id + event_type + ledger range queries
CREATE INDEX IF NOT EXISTS idx_events_contract_type_ledger 
ON events(contract_id, event_type, ledger DESC);

-- Composite index for event_type + ledger range queries (global queries)
CREATE INDEX IF NOT EXISTS idx_events_type_ledger 
ON events(event_type, ledger DESC);

-- Partial index for the most common event type (contract events)
CREATE INDEX IF NOT EXISTS idx_events_contract_type_partial 
ON events(ledger DESC) 
WHERE event_type = 'contract';
