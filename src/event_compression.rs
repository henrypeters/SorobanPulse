use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde_json::Value;
use sqlx::PgPool;
use std::io::{Read, Write};

/// Gzip-compress the JSON representation of `value`.
pub fn compress(value: &Value) -> Result<Vec<u8>, std::io::Error> {
    let json_bytes = serde_json::to_vec(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&json_bytes)?;
    encoder.finish()
}

/// Decompress gzip-compressed bytes back to a JSON `Value`.
pub fn decompress(bytes: &[u8]) -> Result<Value, std::io::Error> {
    let mut decoder = GzDecoder::new(bytes);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;
    serde_json::from_slice(&decompressed)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[derive(sqlx::FromRow)]
struct EventRow {
    id: uuid::Uuid,
    event_data: serde_json::Value,
}

/// Migrate existing uncompressed events to gzip-compressed format.
/// Processes rows in batches to avoid long-running transactions.
pub async fn migrate_existing_events(pool: &PgPool, batch_size: i64) -> anyhow::Result<u64> {
    let mut total_migrated: u64 = 0;
    loop {
        let rows: Vec<EventRow> = sqlx::query_as::<_, EventRow>(
            "SELECT id, event_data FROM events
             WHERE event_data_compressed IS NULL
             LIMIT $1",
        )
        .bind(batch_size)
        .fetch_all(pool)
        .await?;

        if rows.is_empty() {
            break;
        }

        for row in &rows {
            let compressed = match compress(&row.event_data) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(event_id = %row.id, error = %e, "skipping compression for event");
                    continue;
                }
            };
            let original_len = row.event_data.to_string().len();
            sqlx::query(
                "UPDATE events
                 SET event_data_compressed = $1, compression_algo = 'gzip'
                 WHERE id = $2",
            )
            .bind(&compressed)
            .bind(row.id)
            .execute(pool)
            .await?;
            crate::metrics::record_compression_ratio(original_len, compressed.len());
            total_migrated += 1;
        }
    }
    Ok(total_migrated)
}

/// Read the canonical event_data for a row, preferring the compressed column.
/// Returns `None` if decompression fails (with a metric bump and warning).
pub fn read_event_data(
    compressed: Option<&[u8]>,
    algo: Option<&str>,
    plain: &Value,
) -> Value {
    if let Some(bytes) = compressed {
        if algo == Some("gzip") {
            match decompress(bytes) {
                Ok(v) => return v,
                Err(e) => {
                    tracing::warn!(error = %e, "decompression failed, falling back to plain event_data");
                    crate::metrics::record_decompression_failure();
                }
            }
        }
    }
    plain.clone()
}
