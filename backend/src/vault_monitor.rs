use crate::{config::Config, session_manager::SessionManager};
use anyhow::Result;
use sqlx::{Pool, Postgres};
use tokio::time::{self, Duration};
use tracing::info;

pub struct VaultMonitor {
    pool: Pool<Postgres>,
    cfg: Config,
}

impl VaultMonitor {
    pub fn new(pool: Pool<Postgres>, cfg: Config) -> Self {
        Self { pool, cfg }
    }

    pub async fn run(self) -> Result<()> {
        let mut interval = time::interval(Duration::from_secs(30));
        let session_manager = SessionManager::new(self.pool.clone(), self.cfg.clone());

        loop {
            interval.tick().await;
            // For brevity we only log; a real implementation would:
            // * Query active sessions from DB
            // * Check on-chain vault state for expiry/balance
            // * Trigger cleanup_vault transactions when needed
            info!("vault_monitor_heartbeat");

            let _ = &session_manager; // silence unused for now
        }
    }
}
