use crate::{
    auto_deposit::PriorityLevel,
    config::Config,
    delegation_manager::DelegationManager,
    session_manager::{Session, SessionManager},
};
use anyhow::Result;
use axum::{
    extract::{Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub db: Pool<Postgres>,
    pub cfg: Config,
    pub tx_events: broadcast::Sender<SessionEvent>,
}

impl AppState {
    pub async fn new(db: Pool<Postgres>, cfg: Config) -> Result<Self> {
        let (tx_events, _rx) = broadcast::channel(1024);
        Ok(Self { db, cfg, tx_events })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum SessionEvent {
    Created(Session),
    Active(Session),
    Revoked(Session),
    Expired(Session),
}

pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub parent_wallet: String,
    pub session_duration_secs: i64,
    pub max_deposit_lamports: u64,
}

#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session: Session,
    pub ephemeral_wallet: String,
}

pub async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Response, StatusCode> {
    let parent_wallet = req
        .parent_wallet
        .parse()
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let sm = SessionManager::new(state.db.clone(), state.cfg.clone());
    let (session, ephemeral_kp) = sm
        .create_session(parent_wallet, req.session_duration_secs, req.max_deposit_lamports)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let _ = state
        .tx_events
        .send(SessionEvent::Created(session.clone()));

    let resp = CreateSessionResponse {
        session,
        ephemeral_wallet: ephemeral_kp.pubkey().to_string(),
    };

    Ok((StatusCode::OK, Json(resp)).into_response())
}

#[derive(Debug, Deserialize)]
pub struct ApproveSessionRequest {
    pub session_id: Uuid,
    pub vault_pubkey: String,
}

pub async fn approve_session(
    State(state): State<AppState>,
    Json(req): Json<ApproveSessionRequest>,
) -> Result<Response, StatusCode> {
    let vault_pubkey = req
        .vault_pubkey
        .parse()
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let sm = SessionManager::new(state.db.clone(), state.cfg.clone());
    sm.mark_active(req.session_id, vault_pubkey)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Ok(Some(session)) = sm.get(req.session_id).await {
        let _ = state.tx_events.send(SessionEvent::Active(session.clone()));
        Ok((StatusCode::OK, Json(session)).into_response())
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[derive(Debug, Deserialize)]
pub struct RevokeSessionRequest {
    pub session_id: Uuid,
}

pub async fn revoke_session(
    State(state): State<AppState>,
    Json(req): Json<RevokeSessionRequest>,
) -> Result<Response, StatusCode> {
    let sm = SessionManager::new(state.db.clone(), state.cfg.clone());
    sm.revoke(req.session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Ok(Some(session)) = sm.get(req.session_id).await {
        let _ = state.tx_events.send(SessionEvent::Revoked(session.clone()));
        Ok((StatusCode::OK, Json(session)).into_response())
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[derive(Debug, Deserialize)]
pub struct SessionStatusQuery {
    pub session_id: Uuid,
}

pub async fn session_status(
    State(state): State<AppState>,
    Query(q): Query<SessionStatusQuery>,
) -> Result<Response, StatusCode> {
    let sm = SessionManager::new(state.db.clone(), state.cfg.clone());
    if let Ok(Some(session)) = sm.get(q.session_id).await {
        Ok((StatusCode::OK, Json(session)).into_response())
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[derive(Debug, Deserialize)]
pub struct SessionDepositRequest {
    pub session_id: Uuid,
    pub min_trades_buffer: u64,
    pub priority: PriorityLevel,
}

pub async fn session_deposit(
    State(_state): State<AppState>,
    Json(_req): Json<SessionDepositRequest>,
) -> Result<Response, StatusCode> {
    // For brevity we only accept the request and return 202. A full implementation
    // would orchestrate auto-deposit transactions here.
    Ok((StatusCode::ACCEPTED, "scheduled").into_response())
}

pub async fn session_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| async move {
        use axum::extract::ws::{Message, WebSocket};
        use tokio::select;

        let mut rx = state.tx_events.subscribe();
        let (mut sender, mut _receiver) = socket.split();

        loop {
            let evt = match rx.recv().await {
                Ok(e) => e,
                Err(_) => break,
            };

            let json = match serde_json::to_string(&evt) {
                Ok(j) => j,
                Err(_) => continue,
            };

            if sender
                .send(Message::Text(json))
                .await
                .is_err()
            {
                break;
            }

            select! {
                // Could listen for client messages here if desired.
                default => {}
            }
        }
    })
}
