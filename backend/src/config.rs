use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SolanaConfig {
    pub rpc_url: String,
    pub ws_url: String,
    pub commitment: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfig {
    pub key_encryption_key: String,
    pub jwt_secret: String,
    pub rate_limit_sessions_per_minute: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub listen_addr: String,
    pub database: DatabaseConfig,
    pub solana: SolanaConfig,
    pub security: SecurityConfig,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let listen_addr = std::env::var("EVS_LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
        let database_url = std::env::var("EVS_DATABASE_URL")
            .context("EVS_DATABASE_URL must be set for PostgreSQL connection")?;
        let max_connections: u32 = std::env::var("EVS_DATABASE_MAX_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        let rpc_url = std::env::var("EVS_SOLANA_RPC_URL")
            .unwrap_or_else(|_| "http://localhost:8899".into());
        let ws_url = std::env::var("EVS_SOLANA_WS_URL")
            .unwrap_or_else(|_| "ws://localhost:8900".into());
        let commitment = std::env::var("EVS_SOLANA_COMMITMENT").unwrap_or_else(|_| "confirmed".into());

        let key_encryption_key = std::env::var("EVS_KEY_ENCRYPTION_KEY")
            .context("EVS_KEY_ENCRYPTION_KEY must be set for encrypting ephemeral keys")?;
        let jwt_secret = std::env::var("EVS_JWT_SECRET")
            .context("EVS_JWT_SECRET must be set for API authentication")?;
        let rate_limit_sessions_per_minute: u32 = std::env::var("EVS_RATE_LIMIT_SESSIONS_PER_MINUTE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);

        Ok(Self {
            listen_addr,
            database: DatabaseConfig {
                url: database_url,
                max_connections,
            },
            solana: SolanaConfig {
                rpc_url,
                ws_url,
                commitment,
            },
            security: SecurityConfig {
                key_encryption_key,
                jwt_secret,
                rate_limit_sessions_per_minute,
            },
        })
    }
}
