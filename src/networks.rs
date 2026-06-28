use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Network {
    pub chain_id: String,
    pub display_name: String,
    pub rpc_url: String,
    pub passphrase: String,
    pub is_enabled: bool,
    pub last_ledger: Option<i64>,
    pub health_status: String,
}

/// Return all registered networks ordered by chain_id.
pub async fn list_networks(pool: &PgPool) -> sqlx::Result<Vec<Network>> {
    sqlx::query_as::<_, Network>(
        "SELECT chain_id, display_name, rpc_url, passphrase, is_enabled,
                last_ledger, health_status
         FROM networks
         ORDER BY chain_id",
    )
    .fetch_all(pool)
    .await
}

/// Upsert the health status and last-seen ledger for a chain.
pub async fn update_network_health(
    pool: &PgPool,
    chain_id: &str,
    healthy: bool,
    last_ledger: Option<u64>,
) -> sqlx::Result<()> {
    let status = if healthy { "healthy" } else { "unhealthy" };
    let ledger = last_ledger.map(|l| l as i64);
    sqlx::query(
        "UPDATE networks
         SET health_status = $1,
             last_ledger   = COALESCE($2, last_ledger),
             last_seen_at  = NOW()
         WHERE chain_id = $3",
    )
    .bind(status)
    .bind(ledger)
    .bind(chain_id)
    .execute(pool)
    .await?;
    crate::metrics::update_network_health(chain_id, healthy);
    if let Some(l) = last_ledger {
        crate::metrics::update_network_latest_ledger(chain_id, l);
    }
    Ok(())
}
