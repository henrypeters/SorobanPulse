DROP INDEX IF EXISTS idx_events_uncompressed;
ALTER TABLE events
    DROP COLUMN IF EXISTS compression_algo,
    DROP COLUMN IF EXISTS event_data_compressed;
