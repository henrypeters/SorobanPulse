ALTER TABLE subscriptions
    ADD COLUMN IF NOT EXISTS subscription_type TEXT NOT NULL DEFAULT 'single',
    ADD COLUMN IF NOT EXISTS batch_size INT NOT NULL DEFAULT 10,
    ADD COLUMN IF NOT EXISTS batch_timeout_ms INT NOT NULL DEFAULT 5000;

CREATE INDEX IF NOT EXISTS idx_subscriptions_type
    ON subscriptions(subscription_type);
