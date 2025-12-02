use crate::config::Config;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, signature::Keypair};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    Created,
    Active,
    Revoked,
    Expired,
    Cleaned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub parent_wallet: String,
    pub ephemeral_wallet: String,
    pub vault_pubkey: Option<String>,
    pub status: SessionStatus,
    pub session_start: DateTime<Utc>,
    pub session_expiry: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub max_deposit: u64,
    pub total_deposited: u64,
    pub total_spent: u64,
}

pub struct SessionManager {
    pool: Pool<Postgres>,
    cfg: Config,
}

impl SessionManager {
    pub fn new(pool: Pool<Postgres>, cfg: Config) -> Self {
        Self { pool, cfg }
    }

    pub async fn create_session(
        &self,
        parent_wallet: Pubkey,
        session_duration_secs: i64,
        max_deposit: u64,
    ) -> Result<(Session, Keypair)> {
        let now = Utc::now();
        let expiry = now + Duration::seconds(session_duration_secs);

        let mut rng = OsRng;
        let ephemeral = Keypair::generate(&mut rng);

        let session_id = Uuid::new_v4();

        // For this assessment, we store the ephemeral key encrypted using a simple symmetric
        // scheme (ring AES-GCM). In a production setup this would be an HSM or KMS.
        let encrypted_key = crate::transaction_signer::encrypt_keypair(
            &ephemeral,
            &self.cfg.security.key_encryption_key,
        )?;

        sqlx::query!(
            r#"
            INSERT INTO sessions (
                id,
                parent_wallet,
                ephemeral_wallet,
                vault_pubkey,
                status,
                session_start,
                session_expiry,
                last_activity,
                max_deposit,
                total_deposited,
                total_spent,
                encrypted_ephemeral_key
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
            "#,
            session_id,
            parent_wallet.to_string(),
            ephemeral.pubkey().to_string(),
            Option::<String>::None,
            "CREATED",
            now,
            expiry,
            now,
            max_deposit as i64,
            0_i64,
            0_i64,
            encrypted_key,
        )
        .execute(&self.pool)
        .await?;

        Ok((
            Session {
                id: session_id,
                parent_wallet: parent_wallet.to_string(),
                ephemeral_wallet: ephemeral.pubkey().to_string(),
                vault_pubkey: None,
                status: SessionStatus::Created,
                session_start: now,
                session_expiry: expiry,
                last_activity: now,
                max_deposit,
                total_deposited: 0,
                total_spent: 0,
            },
            ephemeral,
        ))
    }

    pub async fn mark_active(&self, session_id: Uuid, vault_pubkey: Pubkey) -> Result<()> {
        let now = Utc::now();
        sqlx::query!(
            r#"UPDATE sessions
               SET status = 'ACTIVE', vault_pubkey = $2, last_activity = $3
               WHERE id = $1"#,
            session_id,
            vault_pubkey.to_string(),
            now,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn revoke(&self, session_id: Uuid) -> Result<()> {
        let now = Utc::now();
        sqlx::query!(
            r#"UPDATE sessions
               SET status = 'REVOKED', last_activity = $2
               WHERE id = $1"#,
            session_id,
            now,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get(&self, session_id: Uuid) -> Result<Option<Session>> {
        let row = sqlx::query!(
            r#"SELECT
                   id,
                   parent_wallet,
                   ephemeral_wallet,
                   vault_pubkey,
                   status,
                   session_start,
                   session_expiry,
                   last_activity,
                   max_deposit,
                   total_deposited,
                   total_spent
               FROM sessions
               WHERE id = $1"#,
            session_id,
        )
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else { return Ok(None) };

        let status = match row.status.as_str() {
            "CREATED" => SessionStatus::Created,
            "ACTIVE" => SessionStatus::Active,
            "REVOKED" => SessionStatus::Revoked,
            "EXPIRED" => SessionStatus::Expired,
            "CLEANED" => SessionStatus::Cleaned,
            _ => SessionStatus::Created,
        };

        Ok(Some(Session {
            id: row.id,
            parent_wallet: row.parent_wallet,
            ephemeral_wallet: row.ephemeral_wallet,
            vault_pubkey: row.vault_pubkey,
            status,
            session_start: row.session_start,
            session_expiry: row.session_expiry,
            last_activity: row.last_activity,
            max_deposit: row.max_deposit as u64,
            total_deposited: row.total_deposited as u64,
            total_spent: row.total_spent as u64,
        }))
    }
}
