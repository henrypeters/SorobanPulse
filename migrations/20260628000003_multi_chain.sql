-- Issue #609: Multi-chain support — chain_id on events and networks registry.
ALTER TABLE events
    ADD COLUMN IF NOT EXISTS chain_id TEXT NOT NULL DEFAULT 'mainnet';

CREATE INDEX IF NOT EXISTS idx_events_chain_id ON events(chain_id);

-- Combined index for the most common multi-chain query pattern.
CREATE INDEX IF NOT EXISTS idx_events_chain_contract
    ON events(chain_id, contract_id);

-- Network registry: one row per indexable Soroban network.
CREATE TABLE IF NOT EXISTS networks (
    chain_id        TEXT        PRIMARY KEY,
    display_name    TEXT        NOT NULL,
    rpc_url         TEXT        NOT NULL,
    passphrase      TEXT        NOT NULL,
    is_enabled      BOOLEAN     NOT NULL DEFAULT true,
    added_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at    TIMESTAMPTZ,
    last_ledger     BIGINT,
    health_status   TEXT        NOT NULL DEFAULT 'unknown'
);

INSERT INTO networks (chain_id, display_name, rpc_url, passphrase, is_enabled)
VALUES (
    'mainnet',
    'Stellar Mainnet',
    'https://mainnet.stellar.validationcloud.io/v1/soroban/rpc',
    'Public Global Stellar Network ; September 2015',
    true
) ON CONFLICT (chain_id) DO NOTHING;
