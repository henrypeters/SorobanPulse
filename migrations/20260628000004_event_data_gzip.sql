-- Issue #610: Application-level gzip compression column for event_data.
-- event_data_compressed holds the gzip-compressed JSON bytes when compression is enabled.
-- When NULL the plain event_data column is the authoritative source.
ALTER TABLE events
    ADD COLUMN IF NOT EXISTS event_data_compressed BYTEA,
    ADD COLUMN IF NOT EXISTS compression_algo       TEXT;

-- Partial index so the migration worker can quickly find uncompressed rows.
CREATE INDEX IF NOT EXISTS idx_events_uncompressed
    ON events(id)
    WHERE event_data_compressed IS NULL;
