DROP TABLE IF EXISTS push_delivery_log;
DROP TABLE IF EXISTS email_delivery_log;
DROP TABLE IF EXISTS email_send_counters;

ALTER TABLE subscriptions
    DROP COLUMN IF EXISTS device_type,
    DROP COLUMN IF EXISTS push_enabled,
    DROP COLUMN IF EXISTS push_token,
    DROP COLUMN IF EXISTS email_enabled,
    DROP COLUMN IF EXISTS email_address;
