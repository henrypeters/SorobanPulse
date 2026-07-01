//! Issue #266: Bloom filter deduplication pre-filter.
//! Issue #615: Session-level Bloom filter for per-RPC-poll deduplication with ledger-reset.
//!
//! Stores hashes of `(tx_hash, contract_id, event_type)` tuples to skip
//! database inserts for events that are very likely already indexed.
//! False positives cause a missed insert (the DB unique constraint is the
//! authoritative guard); false negatives are impossible by design.
//!
//! ## Deduplication layers
//!
//! 1. **Session Bloom filter** (`SessionBloomFilter`): reset every time a new ledger is
//!    detected. Catches duplicates within a single RPC poll session — e.g. overlapping
//!    cursors returning the same event twice in the same batch.
//! 2. **Persistent Bloom filter** (`EventBloomFilter`): seeded from recent DB rows at
//!    startup; survives across poll cycles. Catches events already persisted to the DB.
//! 3. **DB `ON CONFLICT DO NOTHING`**: the authoritative guard for all cases.

use bloomfilter::Bloom;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashSet;

use crate::metrics;

// ── Issue #615: Session-level Bloom filter ───────────────────────────────────

/// A per-poll-session Bloom filter that resets when a new ledger sequence is detected.
///
/// Unlike `EventBloomFilter` (which is long-lived and seeded from the DB), this filter
/// is scoped to a single indexer poll cycle. It is reset on every new ledger, so its
/// memory footprint is bounded by the number of events in one ledger.
pub struct SessionBloomFilter {
    inner: Mutex<Bloom<String>>,
    /// The ledger sequence at which the filter was last reset.
    current_ledger: Mutex<u64>,
    capacity: usize,
    fp_rate: f64,
}

impl SessionBloomFilter {
    /// Create a session filter sized for `capacity` events per ledger.
    pub fn new(capacity: usize, fp_rate: f64) -> Self {
        let bloom = Bloom::new_for_fp_rate(capacity, fp_rate)
            .expect("Failed to create session bloom filter");
        Self {
            inner: Mutex::new(bloom),
            current_ledger: Mutex::new(0),
            capacity,
            fp_rate,
        }
    }

    /// Check whether this event was already seen in the current session.
    ///
    /// Automatically resets the filter when `ledger` advances beyond the last-seen ledger,
    /// then records the event. Returns `true` (duplicate) only when the same ledger is active.
    pub fn check_and_set(&self, tx_hash: &str, contract_id: &str, event_type: &str, ledger: u64) -> bool {
        let key = format!("{tx_hash}:{contract_id}:{event_type}");

        let mut current = self.current_ledger.lock().expect("session bloom ledger lock poisoned");
        if ledger > *current {
            // New ledger detected — reset the filter.
            let new_bloom = Bloom::new_for_fp_rate(self.capacity, self.fp_rate)
                .expect("Failed to recreate session bloom filter");
            *self.inner.lock().expect("session bloom inner lock poisoned") = new_bloom;
            *current = ledger;
            metrics::record_session_bloom_reset();
        }

        let mut guard = self.inner.lock().expect("session bloom inner lock poisoned");
        if guard.check(&key) {
            metrics::record_session_bloom_hit();
            return true;
        }
        guard.set(&key);
        false
    }
}

/// Thread-safe bloom filter for event deduplication.
pub struct EventBloomFilter {
    inner: Mutex<Bloom<String>>,
    capacity: usize,
    fp_rate: f64,
    /// Issue #627: Separate bloom filter for tracking contract existence
    contract_filter: Mutex<Bloom<String>>,
    /// Issue #627: Exact set of known contracts for fallback
    known_contracts: Mutex<HashSet<String>>,
}

impl EventBloomFilter {
    /// Create a new bloom filter with the given false-positive rate and capacity.
    ///
    /// # Panics
    /// Panics if `fp_rate` is not in (0, 1) or `capacity` is 0.
    pub fn new(capacity: usize, fp_rate: f64) -> Self {
        let bloom = Bloom::new_for_fp_rate(capacity, fp_rate)
            .expect("Failed to create bloom filter: invalid capacity or fp_rate");
        let contract_bloom = Bloom::new_for_fp_rate(capacity / 10, fp_rate)
            .expect("Failed to create contract bloom filter");
        Self {
            inner: Mutex::new(bloom),
            capacity,
            fp_rate,
            contract_filter: Mutex::new(contract_bloom),
            known_contracts: Mutex::new(HashSet::new()),
        }
    }

    /// Build the deduplication key for an event.
    fn key(tx_hash: &str, contract_id: &str, event_type: &str) -> String {
        format!("{tx_hash}:{contract_id}:{event_type}")
    }

    /// Returns `true` if the event was probably already seen (bloom filter hit).
    /// Increments `soroban_pulse_bloom_filter_hits_total` on a hit.
    pub fn check(&self, tx_hash: &str, contract_id: &str, event_type: &str) -> bool {
        let k = Self::key(tx_hash, contract_id, event_type);
        let hit = self
            .inner
            .lock()
            .expect("bloom filter lock poisoned")
            .check(&k);
        if hit {
            metrics::record_bloom_filter_hit();
        }
        hit
    }

    /// Record that an event has been seen.
    pub fn set(&self, tx_hash: &str, contract_id: &str, event_type: &str) {
        let k = Self::key(tx_hash, contract_id, event_type);
        self.inner
            .lock()
            .expect("bloom filter lock poisoned")
            .set(&k);
    }

    /// Seed the filter from a list of `(tx_hash, contract_id, event_type)` tuples.
    /// Used at startup to pre-populate from recent DB rows.
    pub fn seed(&self, entries: impl IntoIterator<Item = (String, String, String)>) {
        let mut guard = self.inner.lock().expect("bloom filter lock poisoned");
        let mut contract_guard = self.contract_filter.lock().expect("contract filter lock poisoned");
        let mut known_contracts = self.known_contracts.lock().expect("known_contracts lock poisoned");
        
        for (tx_hash, contract_id, event_type) in entries {
            let k = Self::key(&tx_hash, &contract_id, &event_type);
            guard.set(&k);
            
            // Track contract existence
            contract_guard.set(&contract_id);
            known_contracts.insert(contract_id);
        }
    }

    /// Issue #627: Check if a contract has any indexed events.
    /// Returns true if the contract is likely to exist (may have false positives).
    pub fn contains_contract(&self, contract_id: &str) -> bool {
        // First check the exact set for fast paths
        if let Ok(known) = self.known_contracts.lock() {
            if known.contains(contract_id) {
                return true;
            }
        }
        
        // Then check the bloom filter
        if let Ok(guard) = self.contract_filter.lock() {
            return guard.check(contract_id);
        }
        
        false
    }

    /// Issue #627: Add a contract to the bloom filter.
    pub fn add_contract(&self, contract_id: &str) {
        if let Ok(mut guard) = self.contract_filter.lock() {
            guard.set(contract_id);
        }
        if let Ok(mut known) = self.known_contracts.lock() {
            known.insert(contract_id.to_string());
        }
    }
}

/// Load recent events from the database and seed the bloom filter.
/// Loads up to `limit` most recent events by ledger descending.
pub async fn seed_from_db(
    filter: &EventBloomFilter,
    pool: &sqlx::PgPool,
    limit: i64,
) -> Result<usize, sqlx::Error> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT tx_hash, contract_id, event_type FROM events ORDER BY ledger DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let count = rows.len();
    filter.seed(rows.into_iter().map(|(tx, cid, et)| (tx, cid, et)));
    Ok(count)
}

/// Persist the bloom filter state to the database.
pub async fn persist_state(
    filter: &EventBloomFilter,
    pool: &sqlx::PgPool,
) -> Result<(), sqlx::Error> {
    let guard = filter.inner.lock().expect("bloom filter lock poisoned");
    let bitmap = guard.bitmap();
    let bitmap_bytes = bitmap.iter().map(|&b| b as i16).collect::<Vec<_>>();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    sqlx::query(
        "INSERT INTO indexer_bloom_state (capacity, fp_rate, bitmap, persisted_at) 
         VALUES ($1, $2, $3, to_timestamp($4))
         ON CONFLICT (id) DO UPDATE SET bitmap = $3, persisted_at = to_timestamp($4)"
    )
    .bind(filter.capacity as i32)
    .bind(filter.fp_rate)
    .bind(bitmap_bytes)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

/// Restore the bloom filter state from the database if available and not stale.
pub async fn restore_state(
    pool: &sqlx::PgPool,
    max_age_secs: i64,
) -> Result<Option<EventBloomFilter>, sqlx::Error> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let row: Option<(i32, f64, Vec<i16>)> = sqlx::query_as(
        "SELECT capacity, fp_rate, bitmap FROM indexer_bloom_state 
         WHERE persisted_at > to_timestamp($1) LIMIT 1"
    )
    .bind(now - max_age_secs)
    .fetch_optional(pool)
    .await?;

    match row {
        Some((capacity, fp_rate, bitmap_bytes)) => {
            let bloom = Bloom::new_for_fp_rate(capacity as usize, fp_rate)
                .expect("Failed to create bloom filter from persisted state");

            // Bitmap restoration is deferred to DB re-seeding; the persisted state
            // is used only to restore capacity/fp_rate parameters.
            let _ = &bitmap_bytes;
            
            let contract_bloom = Bloom::new_for_fp_rate(
                (capacity as usize).max(100) / 10,
                fp_rate,
            )
            .expect("Failed to create contract bloom from persisted state");
            Ok(Some(EventBloomFilter {
                inner: Mutex::new(bloom),
                capacity: capacity as usize,
                fp_rate,
                contract_filter: Mutex::new(contract_bloom),
                known_contracts: Mutex::new(HashSet::new()),
            }))
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_filter() -> EventBloomFilter {
        EventBloomFilter::new(10_000, 0.001)
    }

    #[test]
    fn new_filter_has_no_hits() {
        let f = make_filter();
        assert!(!f.check("tx1", "contract1", "contract"));
    }

    #[test]
    fn set_then_check_returns_true() {
        let f = make_filter();
        f.set("tx1", "contract1", "contract");
        assert!(f.check("tx1", "contract1", "contract"));
    }

    #[test]
    fn different_event_type_not_hit() {
        let f = make_filter();
        f.set("tx1", "contract1", "contract");
        assert!(!f.check("tx1", "contract1", "system"));
    }

    #[test]
    fn different_tx_hash_not_hit() {
        let f = make_filter();
        f.set("tx1", "contract1", "contract");
        assert!(!f.check("tx2", "contract1", "contract"));
    }

    #[test]
    fn seed_populates_filter() {
        let f = make_filter();
        f.seed(vec![
            ("tx1".into(), "c1".into(), "contract".into()),
            ("tx2".into(), "c2".into(), "system".into()),
        ]);
        assert!(f.check("tx1", "c1", "contract"));
        assert!(f.check("tx2", "c2", "system"));
        assert!(!f.check("tx3", "c3", "contract"));
    }

    #[test]
    fn multiple_sets_all_hit() {
        let f = make_filter();
        for i in 0..100u32 {
            f.set(&format!("tx{i}"), "contract1", "contract");
        }
        for i in 0..100u32 {
            assert!(f.check(&format!("tx{i}"), "contract1", "contract"));
        }
    }

    #[test]
    fn filter_stores_capacity_and_fp_rate() {
        let f = EventBloomFilter::new(5000, 0.01);
        assert_eq!(f.capacity, 5000);
        assert_eq!(f.fp_rate, 0.01);
    }

    // ── Issue #615: SessionBloomFilter tests ─────────────────────────────────

    fn make_session_filter() -> SessionBloomFilter {
        SessionBloomFilter::new(10_000, 0.001)
    }

    #[test]
    fn session_filter_first_event_not_duplicate() {
        let f = make_session_filter();
        assert!(!f.check_and_set("tx1", "c1", "contract", 100));
    }

    #[test]
    fn session_filter_second_same_event_is_duplicate() {
        let f = make_session_filter();
        f.check_and_set("tx1", "c1", "contract", 100);
        assert!(f.check_and_set("tx1", "c1", "contract", 100));
    }

    #[test]
    fn session_filter_different_tx_hash_not_duplicate() {
        let f = make_session_filter();
        f.check_and_set("tx1", "c1", "contract", 100);
        assert!(!f.check_and_set("tx2", "c1", "contract", 100));
    }

    #[test]
    fn session_filter_resets_on_new_ledger() {
        let f = make_session_filter();
        // Set in ledger 100
        f.check_and_set("tx1", "c1", "contract", 100);
        assert!(f.check_and_set("tx1", "c1", "contract", 100)); // duplicate

        // Advance to ledger 101 — filter resets, event is no longer cached
        assert!(!f.check_and_set("tx1", "c1", "contract", 101));
    }

    #[test]
    fn session_filter_same_ledger_detects_dups_across_calls() {
        let f = make_session_filter();
        for _ in 0..3 {
            f.check_and_set("txA", "cA", "contract", 50);
        }
        // After the first call above, subsequent ones should all be duplicates.
        // The first call returns false; the next two return true.
        // We can verify by resetting and doing a controlled sequence:
        let f2 = make_session_filter();
        let first = f2.check_and_set("txA", "cA", "contract", 50);
        let second = f2.check_and_set("txA", "cA", "contract", 50);
        assert!(!first);
        assert!(second);
    }
}
