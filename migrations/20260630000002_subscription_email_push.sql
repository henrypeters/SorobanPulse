-- Issue #619: Extend subscriptions with email notification fields
ALTER TABLE subscriptions
    ADD COLUMN IF NOT EXISTS email_address TEXT,
    ADD COLUMN IF NOT EXISTS email_enabled  BOOLEAN NOT NULL DEFAULT false;

-- Issue #620: Push notification support
ALTER TABLE subscriptions
    ADD COLUMN IF NOT EXISTS push_token   TEXT,
    ADD COLUMN IF NOT EXISTS push_enabled BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS device_type  TEXT;   -- 'android' | 'ios' | 'web'

-- Issue #619: Rate limiting — track daily email sends per address
CREATE TABLE IF NOT EXISTS email_send_counters (
    email_address TEXT        NOT NULL,
    date_utc      DATE        NOT NULL DEFAULT CURRENT_DATE,
    send_count    INT         NOT NULL DEFAULT 0,
    PRIMARY KEY (email_address, date_utc)
);

-- Issue #619: Email delivery tracking
CREATE TABLE IF NOT EXISTS email_delivery_log (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    subscription_id UUID        REFERENCES subscriptions(id) ON DELETE SET NULL,
    email_address   TEXT        NOT NULL,
    subject         TEXT        NOT NULL,
    status          TEXT        NOT NULL DEFAULT 'pending', -- pending | sent | failed | rate_limited
    error           TEXT,
    sent_at         TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_email_delivery_log_sub
    ON email_delivery_log(subscription_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_email_delivery_log_address
    ON email_delivery_log(email_address, created_at DESC);

-- Issue #620: Push delivery tracking
CREATE TABLE IF NOT EXISTS push_delivery_log (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    subscription_id UUID        REFERENCES subscriptions(id) ON DELETE SET NULL,
    push_token      TEXT        NOT NULL,
    device_type     TEXT,
    status          TEXT        NOT NULL DEFAULT 'pending', -- pending | sent | failed | invalid_token
    error           TEXT,
    sent_at         TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_push_delivery_log_sub
    ON push_delivery_log(subscription_id, created_at DESC);
