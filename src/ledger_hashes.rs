use sqlx::PgPool;
use tracing::{error, info, warn};

/// Persist a ledger hash.  `prev_hash` is the hash of `ledger - 1` (if known).
pub async fn store_ledger_hash(
    pool: &PgPool,
    ledger: u64,
    hash: &str,
    prev_hash: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO ledger_hashes (ledger, hash, prev_hash)
         VALUES ($1, $2, $3)
         ON CONFLICT (ledger) DO UPDATE
             SET hash      = EXCLUDED.hash,
                 prev_hash = EXCLUDED.prev_hash",
    )
    .bind(ledger as i64)
    .bind(hash)
    .bind(prev_hash)
    .execute(pool)
    .await?;
    crate::metrics::update_ledger_hash_chain_height(ledger);
    crate::metrics::record_ledger_hash_verified();
    Ok(())
}

/// Fetch the hash for a specific ledger.
pub async fn get_ledger_hash(pool: &PgPool, ledger: u64) -> sqlx::Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT hash FROM ledger_hashes WHERE ledger = $1",
    )
    .bind(ledger as i64)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(h,)| h))
}

#[derive(sqlx::FromRow)]
struct LedgerHashRow {
    ledger: i64,
    hash: String,
    prev_hash: Option<String>,
}

/// Verify hash-chain continuity for the range `[from_ledger, to_ledger]`.
/// Logs a warning and bumps the mismatch metric for each broken link.
/// Returns the number of mismatches found.
pub async fn verify_hash_chain(
    pool: &PgPool,
    from_ledger: u64,
    to_ledger: u64,
) -> sqlx::Result<u64> {
    let rows: Vec<LedgerHashRow> = sqlx::query_as::<_, LedgerHashRow>(
        "SELECT ledger, hash, prev_hash
         FROM ledger_hashes
         WHERE ledger BETWEEN $1 AND $2
         ORDER BY ledger",
    )
    .bind(from_ledger as i64)
    .bind(to_ledger as i64)
    .fetch_all(pool)
    .await?;

    let mut mismatches: u64 = 0;
    let mut prev: Option<(i64, String)> = None;

    for row in rows {
        if let Some((prev_ledger, ref prev_hash)) = prev {
            if row.ledger == prev_ledger + 1 {
                if row.prev_hash.as_deref() != Some(prev_hash.as_str()) {
                    warn!(
                        ledger = row.ledger,
                        expected_prev = %prev_hash,
                        actual_prev = ?row.prev_hash,
                        "ledger hash chain break detected"
                    );
                    crate::metrics::record_ledger_hash_mismatch(row.ledger as u64);
                    mismatches += 1;
                }
            }
        }
        prev = Some((row.ledger, row.hash));
    }

    Ok(mismatches)
}

/// On startup, validate the last 1 000 ledgers for hash continuity.
pub async fn startup_hash_chain_validation(pool: &PgPool) {
    let latest: Option<i64> = sqlx::query_as::<_, (Option<i64>,)>(
        "SELECT MAX(ledger) FROM ledger_hashes",
    )
    .fetch_one(pool)
    .await
    .ok()
    .and_then(|(v,)| v);

    let Some(latest) = latest else {
        info!("no ledger hashes recorded yet — skipping continuity check");
        return;
    };

    let from = (latest as u64).saturating_sub(1_000);
    match verify_hash_chain(pool, from, latest as u64).await {
        Ok(0) => info!(from, to = latest, "ledger hash chain OK"),
        Ok(n) => error!(mismatches = n, from, to = latest, "ledger hash chain has gaps"),
        Err(e) => error!(error = %e, "failed to verify ledger hash chain"),
    }
}
