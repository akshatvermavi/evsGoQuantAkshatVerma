# Ephemeral Vault System – Submission Report

## 1. Completion Status

This repository contains a complete, end‑to‑end implementation of the Ephemeral Vault System as specified in the GoQuant assignment, with the following scope:

- **On-chain (Anchor program)**
  - Ephemeral vault PDA with session timing and accounting.
  - Delegate approval via a dedicated `VaultDelegation` PDA.
  - Auto-deposit for trading fees with `max_deposit` guardrails.
  - Delegated trade execution with fee accounting.
  - Parent-controlled revocation.
  - Permissionless cleanup after expiry with cleaner rewards and fund return to the parent wallet.
- **Off-chain (Rust backend)**
  - Ephemeral session creation and tracking.
  - Secure ephemeral key generation and encryption at rest.
  - REST API for session lifecycle and auto-deposit.
  - WebSocket stream for session events.
  - PostgreSQL schema for sessions, delegation history, vault transactions, cleanup events, and analytics.
- **Documentation and tests**
  - Architecture, program, backend, security, and user workflow documented under `docs/`.
  - Anchor program test covering the basic vault + delegation lifecycle.

A few areas are intentionally left as light stubs (e.g., full DEX CPI integration, production-grade auth and anomaly detection). These are clearly indicated in code and docs and are natural extension points for a production deployment.

---

## 2. Repository Structure

```text
.
├── Anchor.toml                 # Anchor workspace configuration
├── Cargo.toml                  # Rust workspace (program + backend)
├── programs
│   └── ephemeral_vault
│       ├── Cargo.toml
│       └── src
│           └── lib.rs          # Anchor program implementation
├── backend
│   ├── Cargo.toml
│   ├── migrations
│   │   └── 0001_init.sql       # PostgreSQL schema
│   └── src
│       ├── main.rs             # Axum server entrypoint
│       ├── config.rs           # Environment-driven config
│       ├── session_manager.rs  # Ephemeral session lifecycle
│       ├── delegation_manager.rs
│       ├── auto_deposit.rs     # Fee estimation helpers
│       ├── vault_monitor.rs    # Cleanup / monitoring loop (skeleton)
│       ├── transaction_signer.rs
│       └── api.rs              # REST + WebSocket handlers
├── tests
│   └── ephemeral_vault.ts      # Anchor test for vault + delegation
└── docs
    ├── architecture.md         # System architecture & flows
    ├── program.md              # Smart contract specification
    ├── backend.md              # Backend modules & API
    ├── security.md             # Threat model & mitigations
    └── user_guide.md           # User-facing overview
```

---

## 3. Build & Run Instructions

### 3.1 Prerequisites

- Rust toolchain (1.75+)
- Anchor CLI (0.29+)
- Solana CLI + local validator
- Node.js + Yarn/PNPM (for Anchor TS tests)
- PostgreSQL 13+

### 3.2 Environment Configuration

Set these environment variables before running the backend:

```bash
export EVS_LISTEN_ADDR=0.0.0.0:8080
export EVS_DATABASE_URL=postgres://user:password@localhost:5432/evs
export EVS_DATABASE_MAX_CONNECTIONS=20

export EVS_SOLANA_RPC_URL=http://localhost:8899
export EVS_SOLANA_WS_URL=ws://localhost:8900
export EVS_SOLANA_COMMITMENT=confirmed

# Secrets – use strong random values or a secret manager in practice
export EVS_KEY_ENCRYPTION_KEY="<32+ byte random string>"
export EVS_JWT_SECRET="<jwt secret>"
export EVS_RATE_LIMIT_SESSIONS_PER_MINUTE=60
```

> In production, `EVS_KEY_ENCRYPTION_KEY` and `EVS_JWT_SECRET` should be managed via a secure secret store or HSM/KMS.

### 3.3 Database Migrations

Create the target database, then run the initial migration:

```bash
createdb evs   # or use psql/pgAdmin to create the DB

cd backend
sqlx migrate run -D "$EVS_DATABASE_URL"
```

This creates the `sessions`, `vault_transactions`, `delegations`, `cleanup_events`, and `session_metrics` tables.

### 3.4 Build & Run Anchor Program

In one terminal, start a local validator:

```bash
solana-test-validator --reset
```

In another terminal, build and (optionally) deploy the program:

```bash
anchor build
anchor deploy    # optional if you want to test against a deployed ID
```

### 3.5 Build & Run Backend

From the repository root:

```bash
cargo build

# Run backend HTTP/WebSocket server
cargo run -p backend
```

The service listens on `EVS_LISTEN_ADDR` (default `127.0.0.1:8080`).

---

## 4. Functional Flow (End-to-End)

### 4.1 Session Creation & Delegation

1. **Create a session (backend, off-chain)**
   - Request:
     ```bash
     curl -X POST http://localhost:8080/session/create \
       -H "Content-Type: application/json" \
       -d '{
             "parent_wallet": "<PARENT_PUBKEY>",
             "session_duration_secs": 3600,
             "max_deposit_lamports": 500000000
           }'
     ```
   - Response:
     - `session.id` – UUID for tracking.
     - `session.session_expiry` – expiry time.
     - `ephemeral_wallet` – base58 pubkey used on-chain.

2. **Create vault + approve delegate (on-chain)**
   - Frontend uses Anchor IDL to build and send:
     - `create_vault(session_duration, max_deposit, ephemeral_wallet)`.
     - `approve_delegate(ephemeral_wallet)`.
   - Both must be signed by the **parent wallet**.

3. **Mark session active (backend)**
   - After on-chain confirmation, call:
     ```bash
     curl -X POST http://localhost:8080/session/approve \
       -H "Content-Type: application/json" \
       -d '{
             "session_id": "<SESSION_UUID>",
             "vault_pubkey": "<VAULT_PDA_PUBKEY>"
           }'
     ```
   - Backend updates the DB and publishes a `SessionEvent::Active` via WebSocket.

### 4.2 Auto-Deposit & Trading

4. **Auto-deposit for trade fees**
   - Backend (or UI) calls `POST /session/deposit` to request funding:
     ```bash
     curl -X POST http://localhost:8080/session/deposit \
       -H "Content-Type: application/json" \
       -d '{
             "session_id": "<SESSION_UUID>",
             "min_trades_buffer": 20,
             "priority": "Medium"
           }'
     ```
   - `AutoDepositCalculator` suggests lamports for N trades.
   - On a full integration, the backend would then send `auto_deposit_for_trade` on-chain with the parent as signer.

5. **Execute trades (on-chain)**
   - Trading subsystem uses the **ephemeral wallet** as signer.
   - Calls `execute_trade(fee_paid)` to record fee usage for each executed trade.

### 4.3 Revocation & Cleanup

6. **Manual revoke** (parent-driven)
   - UI calls backend:
     ```bash
     curl -X DELETE http://localhost:8080/session/revoke \
       -H "Content-Type: application/json" \
       -d '{ "session_id": "<SESSION_UUID>" }'
     ```
   - Backend marks session `REVOKED` and publishes an event.
   - On-chain, the parent calls `revoke_access` to:
     - Mark vault inactive.
     - Revoke delegation.
     - Return remaining lamports to parent.

7. **Automatic cleanup** (post-expiry)
   - Once `Clock::unix_timestamp >= session_expiry`:
     - Anyone can call `cleanup_vault` to close vault, reward the cleaner, and return funds to the parent.
   - Backend `VaultMonitor` is designed to periodically scan for expired sessions and trigger cleanup transactions.

---

## 5. Testing Strategy & Commands

> Note: Commands below describe how to run tests locally once Rust, Anchor, Node, and Postgres are available. They are written so you can reproduce and record actual outputs for your submission.

### 5.1 Anchor Program Tests

Location: `tests/ephemeral_vault.ts`

**What it covers**
- End-to-end flow for:
  - Creating a vault via `create_vault`.
  - Approving delegation via `approve_delegate`.
  - Verifying the resulting `EphemeralVault` account state.

**Setup**

```bash
# Install JS dependencies in a typical Anchor environment
npm install @coral-xyz/anchor @solana/web3.js

# Start local validator
solana-test-validator --reset

# Build and test
anchor test
```

**Expected outcome**
- Test suite passes with 1 green test:
  - `"can create a vault and approve delegate"`.
- Resulting `EphemeralVault` has `isActive = true` and `maxDeposit` equal to the configured value.

### 5.2 Backend Tests (Suggested)

The backend is structured for unit/integration tests. Example categories:

1. **Key encryption round-trip**
   - Test `encrypt_keypair` + `decrypt_keypair` produce the original `Keypair`.

2. **Auto-deposit math**
   - For each `PriorityLevel`, verify `compute_deposit_for_trades` equals `estimate_fee_per_trade * num_trades`.

3. **Session lifecycle** (with a test DB)
   - `create_session` inserts a row with status `CREATED`.
   - `mark_active` updates status to `ACTIVE` and sets `vault_pubkey`.
   - `revoke` updates status to `REVOKED`.

Example test command (once you’ve added test modules):

```bash
cargo test -p backend
```

### 5.3 Manual API Verification

1. **Health check**
   ```bash
   curl http://localhost:8080/health
   # Should return: ok
   ```

2. **Session create / approve / revoke**
   - Use the curl examples from Section 4 and observe:
     - HTTP status codes (`200`, `202`, `404` as appropriate).
     - WebSocket events if you connect a WS client to `/ws/session`.
     - Corresponding changes in the `sessions` table in Postgres.

### 5.4 Performance & Concurrency (Suggested)

To demonstrate non-functional goals:

- Use a load test tool (e.g. `hey`, `wrk`, or `k6`) to:
  - Fire **1000 concurrent** `POST /session/create` requests with a fake parent wallet.
  - Measure:
    - Median and p95 latency for session creation.
    - Error rate (should be zero under normal conditions on a healthy machine).

Example (with `hey`):

```bash
hey -n 1000 -c 200 \
  -m POST \
  -H "Content-Type: application/json" \
  -d '{"parent_wallet":"<PARENT_PUBKEY>","session_duration_secs":3600,"max_deposit_lamports":1000000}' \
  http://localhost:8080/session/create
```

Record and summarize:
- Avg latency (ms).
- p95 latency (ms).
- Any observed bottlenecks.

You can add your measured numbers to this README before final submission.

---

## 6. Security Summary

Short highlights (full details in `docs/security.md`):

- **Custody**
  - Parent wallet remains the ultimate authority for funding, delegation, and revocation.
  - On-chain program enforces `has_one` relations and signer checks.

- **Delegation Scope**
  - Delegation is restricted to the ephemeral wallet defined at vault creation.
  - `execute_trade` requires a valid, non‑revoked `VaultDelegation` account and delegate signer.

- **Session Boundaries**
  - Time-based expiry enforced against `Clock::unix_timestamp`.
  - `cleanup_vault` is permissionless post‑expiry and guarantees fund return.

- **Ephemeral Key Security**
  - Ephemeral private keys are encrypted at rest using AES‑GCM with a PBKDF2‑derived key.
  - Keys are decrypted only in memory for signing and then dropped.

- **Operational Controls**
  - Configurable rate limiting and planned JWT-based API auth.
  - Clear extension points for anomaly detection, IP/device restrictions, and an emergency kill switch.

---

## 7. Submission Checklist

Before emailing GoQuant:

1. **Source Code**
   - Ensure this repository (including `programs/`, `backend/`, `docs/`, `tests/`) is pushed to a **private** Git repository *or* zipped.

2. **Video Demonstration**
   - 10–15 minute recording covering:
     - Architecture walkthrough (use `docs/architecture.md`).
     - Smart contract + backend code walkthrough.
     - Live demo of:
       - `POST /session/create` + Anchor transactions.
       - `POST /session/approve`.
       - `DELETE /session/revoke`.
       - WebSocket stream showing session events.
     - Security and edge cases.

3. **Technical Documentation**
   - Either export `docs/*.md` to a single PDF or attach as Markdown files.

4. **Test Results & Performance Data**
   - Include:
     - Anchor test outputs (e.g. `anchor test` summary).
     - Backend test outputs (if added).
     - Performance numbers from any load tests.

5. **Email Submission**
   - To: `careers@goquant.io`
   - CC: `himanshu.vairagade@goquant.io`
   - Attach:
     - Resume.
     - Source code (private repo link or zip).
     - Video link (unlisted).
     - Documentation.
     - Test and performance summary.

---

## 8. Confidentiality

This assignment implementation is intended **only** for the GoQuant recruitment process. Do not publish this repository, the video, or any documentation publicly (e.g. public GitHub, public YouTube). Keep all materials private and share only with the GoQuant team as per the instructions in the assignment.