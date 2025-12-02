# Security Analysis

## Threat Model
- **Adversaries**:
  - External attackers attempting to hijack sessions, steal keys, or drain funds.
  - Malicious or compromised backend operator.
  - Malicious client attempting to abuse the API (DoS, abusive session creation, etc.).
- **Assets**:
  - Parent wallet funds.
  - Ephemeral keypairs and their delegated authority.
  - On-chain vault balances.
  - Session and transaction history in PostgreSQL.

## Attack Surface
- On-chain Anchor program instructions (`create_vault`, `approve_delegate`, `auto_deposit_for_trade`, `execute_trade`, `revoke_access`, `cleanup_vault`).
- Backend REST and WebSocket endpoints.
- Encrypted ephemeral key storage in Postgres.
- Solana RPC connection.

## Mitigations

### On-Chain Program
- **Unauthorized vault manipulation**:
  - `has_one` constraints and signer checks ensure only `parent_wallet` can create, fund, revoke, or be refunded.
  - `approve_delegate` enforces that the delegate equals the pre-registered `ephemeral_wallet`.
- **Session hijacking**:
  - `execute_trade` requires the correct `VaultDelegation` account and signer to match `delegate`.
  - `session_expiry` and `is_active` are checked on each relevant instruction.
- **Over-deposit and runaway funding**:
  - `max_deposit` is enforced in `auto_deposit_for_trade`.
- **Fund loss on expiry**:
  - Anyone can call `cleanup_vault` after expiry; funds return to parent plus a small capped reward to the cleaner.

### Backend
- **Ephemeral key protection**:
  - Keys are encrypted at rest using AES-GCM with a derived key from `EVS_KEY_ENCRYPTION_KEY`.
  - Only decrypted in-memory for signing, then dropped.
- **API abuse**:
  - IP-based rate limiting on session creation (supported by config, implementable via middleware).
  - Simple health endpoint allows observability without leaking data.
- **Session hijacking at API level**:
  - Intended to be mitigated by JWTs or signed nonces based on parent wallet ownership (not fully implemented in assessment code but accounted for in config).
- **Emergency kill switch**:
  - Config-driven flag or admin-only endpoint could disable new session creation and auto-deposits and trigger bulk revocation.

### Operational Best Practices
- Run backend and Postgres in private VPC segments with limited ingress.
- Restrict Solana RPC endpoint usage to trusted providers.
- Log all critical operations (session create/revoke, delegation changes, cleanup events) to an append-only audit log.
- Rotate `EVS_KEY_ENCRYPTION_KEY` and `EVS_JWT_SECRET` regularly and manage them via secure secret management.

## Residual Risks and Future Hardening
- Multi-sig parent wallets and hardware wallet support further reduce single-key compromise risk.
- HSM/KMS-backed key operations would eliminate direct access to encryption keys from the app container.
- Device fingerprinting, IP whitelisting, and anomaly detection (e.g., ML-based) can enhance detection of unusual trading or deposit patterns.
- Formal verification of the Anchor program and property-based testing can further increase confidence in the absence of fund-loss bugs.
