# Ephemeral Vault System Architecture
The Ephemeral Vault System consists of three major components: an Anchor on-chain program, an off-chain Rust backend service, and a PostgreSQL database used for durable session/vault tracking and analytics. Together they enable gasless, session-scoped trading through ephemeral wallets while keeping custody and control with the parent wallet.

## Components Overview
- **Anchor program (`ephemeral_vault`)**
  - Manages lifecycle of PDA-based `EphemeralVault` accounts and associated `VaultDelegation` accounts.
  - Enforces session expiry, delegation rules, and fund movement from vault PDA back to parent wallet.
  - Emits rich events for each operation (`VaultCreated`, `DelegateApproved`, `AutoDeposit`, `TradeExecuted`, `AccessRevoked`, `VaultCleaned`).
- **Rust backend (`backend`)**
  - Exposes REST + WebSocket APIs for frontend clients.
  - Generates and encrypts ephemeral keypairs, manages session records, orchestrates on-chain instructions, and monitors vaults.
  - Integrates with Solana RPC for transaction submission and on-chain state verification.
- **PostgreSQL**
  - Persists sessions, vault transactions, delegation history, cleanup events, and per-session metrics.
  - Provides a source of truth for monitoring, analytics, anomaly detection, and auditability.

## Session Lifecycle (High Level)
1. **User connects & session creation**
   - Frontend calls `POST /session/create` with the parent wallet pubkey, desired session duration and `max_deposit`.
   - Backend `SessionManager` generates an ephemeral keypair and encrypts it using a KEK from env/HSM.
   - Backend persists a `sessions` row with status `CREATED` and returns:
     - Session id (UUID).
     - Ephemeral public key for UI to show/track.
   - Frontend uses Anchor IDL to construct and sign `create_vault` and `approve_delegate` transactions from the parent wallet.
2. **Delegation approval**
   - After the on-chain transactions confirm, frontend calls `POST /session/approve` with the session id and vault PDA.
   - Backend marks the session `ACTIVE` and optionally verifies the `VaultDelegation` account on-chain.
3. **Auto-deposit and trading**
   - Before a burst of trades, backend (or frontend via `POST /session/deposit`) uses `AutoDepositCalculator` to size a SOL deposit for expected trades.
   - Backend builds and submits `auto_deposit_for_trade` against the Anchor program, funding the vault PDA up to `max_deposit`.
   - Trades are executed via a dark pool DEX integration where the ephemeral wallet signs, while the vault PDA provides margin/fees through the program.
   - `execute_trade` records fee usage into `total_spent` to support risk limits and analytics.
4. **Revocation and cleanup**
   - At any point, the parent wallet can call `revoke_access`, which marks the vault inactive, revokes delegation and returns available lamports to the parent.
   - Once `session_expiry` passes, anyone can call `cleanup_vault` to:
     - Mark the vault inactive (if still active).
     - Return remaining lamports to parent wallet.
     - Pay a small reward to the cleaner.
     - Close the vault account to reclaim rent.
   - Backend `VaultMonitor` also watches for expired sessions and triggers cleanup transactions.

## Fund Flow (Conceptual)
- **Parent wallet → Vault PDA**
  - Funded via `auto_deposit_for_trade` (SystemProgram transfer from parent signer to vault PDA).
  - `EphemeralVault.total_deposited` tracks total lamports that ever flowed from parent into the vault.
- **Vault PDA → DEX / fees**
  - In a full DEX integration, `execute_trade` would perform CPI calls that move lamports/tokens from the vault PDA to margin accounts and fee destinations.
  - The assessment version models this by updating `total_spent` and emitting `TradeExecuted` events.
- **Vault PDA → Parent wallet + cleaner reward**
  - On `revoke_access` and `cleanup_vault`, remaining lamports (minus rent minimum and a capped reward) are transferred back to the parent wallet.
  - `cleanup_vault` additionally pays a small lamport reward to the cleanup caller.

## Security Model (Summary)
- The parent wallet is the ultimate authority:
  - Only the parent can create vaults, approve delegates, and revoke access.
  - `has_one` constraints and signer requirements enforce this on-chain.
- Delegation is scoped to trading only:
  - `VaultDelegation` links a specific `EphemeralVault` and delegate pubkey.
  - `execute_trade` accepts only the recorded delegate as signer and only while the session is active and not expired.
- Session expiry is enforced on-chain via `Clock` sysvar checks in each relevant instruction.
- Backend never stores raw private keys in plaintext:
  - Ephemeral keypairs are encrypted at rest using AES-GCM (via `ring`) with a KEK supplied via environment variables or HSM.

## Component Interactions
- **Frontend ↔ Backend**
  - REST endpoints for lifecycle operations (`/session/create`, `/session/approve`, `/session/revoke`, `/session/deposit`, `/session/status`).
  - WebSocket channel (`/ws/session`) for real-time session/vault state updates.
- **Backend ↔ Anchor Program**
  - DelegationManager builds program instructions using PDAs derived from parent + ephemeral keys.
  - TransactionSigner signs with ephemeral keys (when acting as fee payer/authority) and submits to Solana RPC.
- **Backend ↔ PostgreSQL**
  - SessionManager and other managers read and write session, transaction, delegation, cleanup, and metrics tables using `sqlx`.
- **Anchor Program ↔ Solana Runtime**
  - SystemProgram CPI for lamport transfer.
  - `Rent` and `Clock` sysvars for rent-minimum and time-based checks.

This architecture cleanly separates on-chain custody and enforcement from off-chain orchestration and monitoring, while keeping the UX simple for dark pool traders using ephemeral, session-bound wallets.