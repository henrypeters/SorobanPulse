# Data Retention Policy

This document explains how SorobanPulse stores, retains, archives, and deletes data, and what operators and end-users can expect under GDPR and similar privacy regulations.

## Default Retention Periods

| Data Category | Default Retention | Configurable? | Notes |
|---|---|---|---|
| Indexed contract events | Indefinite | Yes (`EVENT_RETENTION_DAYS`) | Core indexer data; set a value to enable auto-expiry |
| Subscription metadata | Indefinite | No | Deleted when subscription is cancelled |
| Webhook configurations | Indefinite | No | Deleted when webhook is removed |
| Webhook delivery logs | 90 days | Yes (`DELIVERY_LOG_RETENTION_DAYS`) | Includes request/response payloads |
| Notification delivery logs | 90 days | Yes (`NOTIFICATION_LOG_RETENTION_DAYS`) | Per-channel send receipts |
| ABI cache entries | 30 days (TTL) | Yes (`ABI_CACHE_TTL_DAYS`) | Auto-evicted; re-fetched on demand |
| Ledger hash records | Indefinite | Yes | Used for deduplication; safe to truncate after 7 days |
| API access logs | 30 days | Via log aggregator | Controlled by your log infrastructure (see [log-aggregation.md](log-aggregation.md)) |
| Admin audit logs | 365 days | No | Retain for compliance; see [audit_logging.md](audit_logging.md) |

Set retention environment variables in `.env`:

```env
EVENT_RETENTION_DAYS=365
DELIVERY_LOG_RETENTION_DAYS=90
NOTIFICATION_LOG_RETENTION_DAYS=90
ABI_CACHE_TTL_DAYS=30
```

When `EVENT_RETENTION_DAYS` is unset (default), events are kept indefinitely. Set it only if your deployment has storage constraints or a legal obligation to purge historical events.

## Archival Procedures

Archive data before deleting it to preserve auditability.

### Archiving events to cold storage

Use `pg_dump` with a row filter to export events older than your retention window before the scheduled purge job removes them:

```bash
pg_dump \
  --table=events \
  --where="created_at < NOW() - INTERVAL '365 days'" \
  --format=custom \
  --compress=9 \
  --file="events_archive_$(date +%Y%m).pgdump" \
  "$DATABASE_URL"
```

Upload the dump to your object store of choice:

```bash
# AWS S3
aws s3 cp events_archive_$(date +%Y%m).pgdump s3://your-bucket/soroban-pulse/archives/

# GCP Cloud Storage
gsutil cp events_archive_$(date +%Y%m).pgdump gs://your-bucket/soroban-pulse/archives/
```

### Archiving delivery logs

```bash
psql "$DATABASE_URL" -c "\COPY (
  SELECT * FROM delivery_logs WHERE created_at < NOW() - INTERVAL '90 days'
) TO STDOUT CSV HEADER" | gzip > delivery_logs_$(date +%Y%m).csv.gz
```

### Scheduling archive + purge

Add a cron job or Kubernetes `CronJob` that runs the archive script before the purge:

```yaml
# k8s/archive-cronjob.yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: soroban-pulse-archive
spec:
  schedule: "0 2 1 * *"   # 02:00 on the 1st of every month
  jobTemplate:
    spec:
      template:
        spec:
          containers:
            - name: archiver
              image: your-registry/soroban-pulse-archiver:latest
              envFrom:
                - secretRef:
                    name: soroban-pulse-secrets
          restartPolicy: OnFailure
```

## GDPR Compliance Guide

SorobanPulse indexes **on-chain public data** from the Stellar network. Because ledger transactions are inherently public and immutable, the indexed events themselves do not normally constitute personal data under GDPR. However, **subscription metadata and delivery logs may contain personal data** (e.g., email addresses or webhook URLs that could identify a natural person).

### What personal data SorobanPulse holds

| Data | Location | Personally identifiable? |
|---|---|---|
| Contract event payloads | `events` table | Generally no (public blockchain data) |
| Subscriber email addresses | `subscriptions` table | **Yes** |
| Webhook endpoint URLs | `webhooks` table | Potentially (if URL contains user ID) |
| Email delivery logs | `delivery_logs` table | **Yes** (recipient address) |
| API authentication keys | `api_keys` table | Potentially (if linked to a person) |

### Right to Access (Article 15)

Query all personal data for a given email address:

```sql
-- Events are not personal data; focus on subscription metadata
SELECT id, email, created_at, filters FROM subscriptions WHERE email = $1;
SELECT id, endpoint_url, created_at FROM webhooks WHERE owner_email = $1;
SELECT id, channel, recipient, sent_at FROM delivery_logs WHERE recipient = $1;
```

Return the result set to the data subject within 30 days.

### Right to Rectification (Article 16)

Update subscriber contact details:

```sql
UPDATE subscriptions SET email = $new_email WHERE email = $old_email;
UPDATE delivery_logs SET recipient = $new_email WHERE recipient = $old_email;
```

### Right to Erasure (Article 17 — "Right to be Forgotten")

Delete all personal data for a subscriber:

```sql
BEGIN;

-- Remove delivery logs first (foreign key dependency)
DELETE FROM delivery_logs
WHERE subscription_id IN (SELECT id FROM subscriptions WHERE email = $1);

-- Remove webhook delivery logs
DELETE FROM delivery_logs
WHERE webhook_id IN (SELECT id FROM webhooks WHERE owner_email = $1);

-- Remove webhooks
DELETE FROM webhooks WHERE owner_email = $1;

-- Remove subscriptions
DELETE FROM subscriptions WHERE email = $1;

COMMIT;
```

Contract events on the Stellar ledger are immutable public data and are outside the scope of erasure. Inform the data subject of this limitation.

### Right to Portability (Article 20)

Export a subscriber's data as JSON:

```sql
SELECT row_to_json(s) FROM subscriptions s WHERE email = $1;
SELECT row_to_json(w) FROM webhooks w WHERE owner_email = $1;
```

### Data Processing Agreement (DPA)

If you are a SaaS operator running SorobanPulse on behalf of customers, you act as a **data processor** for any personal data your customers supply. Ensure you have a signed DPA with each customer and that your infrastructure providers (cloud, SMTP) also have DPAs in place.

### Data residency

SorobanPulse itself does not enforce data residency. To constrain where personal data is stored:
- Deploy PostgreSQL in the required region.
- Use region-specific SMTP relays for email delivery.
- Configure Kinesis / Pub/Sub / Kafka topics in the compliant region.

## Data Deletion Procedures

### Automated purge via cron

Enable automatic event purging by setting `EVENT_RETENTION_DAYS`. SorobanPulse runs a background worker that deletes expired rows in configurable batch sizes to avoid long-running locks:

```env
EVENT_RETENTION_DAYS=365
PURGE_BATCH_SIZE=5000          # rows deleted per transaction (default: 5000)
PURGE_INTERVAL_HOURS=6         # how often the purge worker runs (default: 6)
```

The purge worker logs each batch:

```
INFO soroban_pulse::purge: deleted 5000 expired events, next_run=2026-07-01T02:00:00Z
```

### Manual one-time deletion

Delete all events older than a specific date:

```sql
-- Dry run first
SELECT COUNT(*) FROM events WHERE created_at < '2025-01-01';

-- Then delete in batches to avoid table lock contention
DO $$
DECLARE
  deleted_count INT;
BEGIN
  LOOP
    DELETE FROM events
    WHERE id IN (
      SELECT id FROM events
      WHERE created_at < '2025-01-01'
      LIMIT 5000
    );
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    EXIT WHEN deleted_count = 0;
    PERFORM pg_sleep(0.1);  -- brief pause between batches
  END LOOP;
END $$;
```

### Delivery log purge

```sql
DELETE FROM delivery_logs WHERE created_at < NOW() - INTERVAL '90 days';
```

Run this during low-traffic windows; add `VACUUM ANALYZE delivery_logs;` afterwards to reclaim disk space.

### Full data wipe (decommission)

To completely remove all SorobanPulse data (e.g., when decommissioning an instance):

```bash
# Drop the entire schema — irreversible
psql "$DATABASE_URL" -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"
```

Revoke database credentials and destroy any object-store archives according to your organisation's data disposal policy.

## Related Documentation

- [Encryption at rest setup guide](encryption.md)
- [Audit logging](audit_logging.md)
- [Log aggregation integration](log-aggregation.md)
- [Backup verification CI workflow](backup-verification.md)
