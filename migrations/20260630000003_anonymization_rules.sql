-- Issue #618: Anonymization rules configuration table.
-- Stores regex-based PII detection patterns and their replacements.
-- When empty, the application falls back to built-in default patterns.

CREATE TABLE IF NOT EXISTS anonymization_rules (
    id          SERIAL PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    pattern     TEXT NOT NULL,
    replacement TEXT NOT NULL DEFAULT '[REDACTED]',
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    description TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE anonymization_rules IS
    'Regex-based PII anonymization rules for event data (issue #618).';

COMMENT ON COLUMN anonymization_rules.pattern IS
    'POSIX/PCRE regex pattern matched against string values in event_data JSON.';

COMMENT ON COLUMN anonymization_rules.replacement IS
    'Replacement string — may include $1 capture-group references.';

-- Pre-populate with the same built-in patterns the Rust module uses,
-- so operators can see and override them via the /v1/config/anonymization API.
INSERT INTO anonymization_rules (name, pattern, replacement, description) VALUES
    ('email',       '[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}',
                    '[REDACTED:email]',     'Email addresses'),
    ('phone',       '\+?[0-9]{7,15}',
                    '[REDACTED:phone]',     'Phone numbers (7–15 digits, optional leading +)'),
    ('ssn',         '\b\d{3}-\d{2}-\d{4}\b',
                    '[REDACTED:ssn]',       'US Social Security Numbers'),
    ('ipv4',        '\b(?:\d{1,3}\.){3}\d{1,3}\b',
                    '[REDACTED:ipv4]',      'IPv4 addresses'),
    ('credit_card', '\b(?:\d[ \-]?){13,16}\b',
                    '[REDACTED:credit_card]', 'Credit / debit card numbers')
ON CONFLICT (name) DO NOTHING;

CREATE INDEX IF NOT EXISTS idx_anonymization_rules_enabled ON anonymization_rules(enabled);
