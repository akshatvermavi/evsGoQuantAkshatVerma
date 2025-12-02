# Backend Service Documentation

## Overview
The backend service is a Rust application built with Axum, SQLx, and the Solana Rust SDK. Its responsibilities are to:
- Manage ephemeral wallet sessions.
- Orchestrate Solana transactions for vault creation, delegation, deposits, and cleanup.
- Expose REST and WebSocket APIs to the trading frontend.
- Persist and query state from PostgreSQL for reliability and analytics.

## Module Architecture
- `main.rs` – Initializes logging, loads configuration, creates a Postgres pool, constructs `AppState`, and starts the Axum HTTP server.
- `config.rs` – Loads environment-driven configuration (listen address, database, Solana RPC endpoints, security settings).
- `session_manager.rs` – Core session lifecycle logic and DB persistence.
- `delegation_manager.rs` – Builds on-chain instructions for `create_vault` and `approve_delegate` and verifies delegation (stubbed for assessment).
- `auto_deposit.rs` – Contains `AutoDepositCalculator` for estimating lamports required per trade and per session.
- `vault_monitor.rs` – Background task skeleton that periodically inspects sessions and can trigger cleanup.
- `transaction_signer.rs` – Encrypts/decrypts ephemeral keypairs and sends signed transactions via Solana RPC.
- `api.rs` – REST + WebSocket handlers and shared `AppState`.

## Key Management Strategy
- Ephemeral keypairs are generated in `SessionManager::create_session` using OS RNG.
- Private keys are serialized and encrypted with AES-256-GCM using a KEK derived from `EVS_KEY_ENCRYPTION_KEY`.
- Encrypted key blobs are stored in the `sessions.encrypted_ephemeral_key` column.
- When a transaction needs to be signed by the ephemeral wallet, the backend would:
  - Fetch the encrypted key from DB.
  - Decrypt via `transaction_signer::decrypt_keypair`.
  - Use `Keypair` to sign the transaction.
- In production this KEK should live in HSM/KMS and rotate regularly.

## REST API Specification

### `POST /session/create`
Creates a new ephemeral session.

**Request body**
```json
{
  "parent_wallet": "<base58 pubkey>",
  "session_duration_secs": 3600,
  "max_deposit_lamports": 500000000
}
```

**Response body**
```json
{
  "session": { /* Session object */ },
  "ephemeral_wallet": "<base58 pubkey>"
}
```

### `POST /session/approve`
Marks a session as active once on-chain delegation is confirmed.

**Request body**
```json
{
  "session_id": "<uuid>",
  "vault_pubkey": "<base58 pubkey>"
}
```

**Response** – `200 OK` with the updated Session, or `404` if unknown.

### `DELETE /session/revoke`
Revokes a session and marks it as `REVOKED` in the DB (on-chain `revoke_access` is orchestrated out-of-band in this assessment).

**Request body**
```json
{
  "session_id": "<uuid>"
}
```

**Response** – `200 OK` with the updated Session, or `404` if unknown.

### `GET /session/status`
Fetches information about a session.

**Query params**
- `session_id` – UUID.

**Response** – `200 OK` with `Session` or `404`.

### `POST /session/deposit`
Placeholder endpoint that would trigger auto-deposit logic.

**Request body**
```json
{
  "session_id": "<uuid>",
  "min_trades_buffer": 20,
  "priority": "Medium"
}
```

**Response** – `202 Accepted` when the request is queued.

## WebSocket API

### `GET /ws/session`
Upgrades to WebSocket and streams `SessionEvent` objects:

```json
{
  "type": "Created" | "Active" | "Revoked" | "Expired",
  "data": { /* Session */ }
}
```

The client can subscribe once and receive updates whenever any session changes; in a production version you would likely filter by `session_id` or user.

## Database Schema
Core schema is defined in `backend/migrations/0001_init.sql`:

- `sessions` – one row per ephemeral trading session.
- `vault_transactions` – records deposits, trade fees, refunds, and cleanup rewards.
- `delegations` – delegation history (who was delegated, when, and if/when it was revoked).
- `cleanup_events` – on-chain cleanup operations and their rewards.
- `session_metrics` – aggregated metrics for analytics.

## Deployment Notes
- **Environment variables** (minimal set):
  - `EVS_LISTEN_ADDR` – e.g. `0.0.0.0:8080`.
  - `EVS_DATABASE_URL` – Postgres connection string.
  - `EVS_DATABASE_MAX_CONNECTIONS` – pool size.
  - `EVS_SOLANA_RPC_URL`, `EVS_SOLANA_WS_URL`, `EVS_SOLANA_COMMITMENT`.
  - `EVS_KEY_ENCRYPTION_KEY` – KEK for ephemeral key encryption.
  - `EVS_JWT_SECRET` – for API auth (not fully wired in assessment code).
  - `EVS_RATE_LIMIT_SESSIONS_PER_MINUTE` – simple rate limit knob.

- **Runtime**: built on Tokio multi-threaded runtime, designed to handle 1000+ concurrent sessions with modest resources.

- **Scaling**: multiple backend instances can run behind a load balancer; all state is shared via Postgres and Solana RPC.

## Limitations in Assessment Version
- Auto-deposit execution and integration with `auto_deposit_for_trade` are sketched but not fully wired.
- On-chain verification of `VaultDelegation` accounts is stubbed out.
- Authentication/authorization is minimized; production system should use signed nonces or JWTs bound to parent wallets.
- VaultMonitor only logs a heartbeat; real implementation would query DB and submit cleanup transactions.

Despite these simplifications, the skeleton demonstrates the intended separation of concerns and provides clear extension points for a full production deployment.