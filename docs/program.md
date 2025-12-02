# EphemeralVault Anchor Program

## PDA Derivation
- **Vault PDA**: `seeds = [b"vault", parent_wallet, ephemeral_wallet]`
  - Stores vault metadata, session timing, and accounting.
- **Delegation PDA**: `seeds = [b"delegation", vault_pubkey]`
  - Stores delegation metadata linking a vault to its delegate.

## Accounts

### EphemeralVault
```text
pub struct EphemeralVault {
    pub parent_wallet: Pubkey,
    pub ephemeral_wallet: Pubkey,
    pub session_start: i64,
    pub session_expiry: i64,
    pub is_active: bool,
    pub total_deposited: u64,
    pub total_spent: u64,
    pub max_deposit: u64,
    pub bump: u8,
}
```
- `parent_wallet` – authority that creates, funds, revokes, and ultimately owns all funds.
- `ephemeral_wallet` – session keypair used by the backend/frontend to sign trading-related transactions.
- `session_start` / `session_expiry` – bounds for when the vault can be used; derived from `Clock` sysvar.
- `is_active` – logical flag which is set to `false` when revoked or cleaned up.
- `total_deposited` – sum of all lamports ever transferred from parent into this vault via `auto_deposit_for_trade`.
- `total_spent` – sum of all lamports accounted as spent by `execute_trade`.
- `max_deposit` – guardrail to prevent over-depositing beyond what the parent approved.
- `bump` – PDA bump for vault derivation.

### VaultDelegation
```text
pub struct VaultDelegation {
    pub vault: Pubkey,
    pub delegate: Pubkey,
    pub approved_at: i64,
    pub revoked_at: Option<i64>,
    pub bump: u8,
}
```
- `vault` – associated `EphemeralVault` PDA.
- `delegate` – delegate pubkey (must equal `EphemeralVault.ephemeral_wallet`).
- `approved_at` – UNIX timestamp when delegation was created.
- `revoked_at` – set when parent revokes delegation.
- `bump` – PDA bump for delegation derivation.

## Instructions

### create_vault
```rust
pub fn create_vault(
    ctx: Context<CreateVault>,
    session_duration: i64,
    max_deposit: u64,
    ephemeral_wallet: Pubkey,
) -> Result<()>
```
- **Accounts**:
  - `parent: Signer` – payer and ultimate authority.
  - `ephemeral_wallet: UncheckedAccount` – off-chain-generated ephemeral wallet.
  - `vault: EphemeralVault (init, seeds = [b"vault", parent, ephemeral_wallet])`.
  - `system_program: System`.
- **Behaviour**:
  - Derives and initializes `EphemeralVault` PDA.
  - Sets session start/expiry based on `Clock` and provided `session_duration`.
  - Sets `max_deposit` and marks vault `is_active = true`.
  - Emits `VaultCreated` event.

### approve_delegate
```rust
pub fn approve_delegate(ctx: Context<ApproveDelegate>, delegate: Pubkey) -> Result<()>
```
- **Accounts**:
  - `vault: EphemeralVault (has_one = parent_wallet)`.
  - `parent: Signer` – must match `EphemeralVault.parent_wallet`.
  - `delegation: VaultDelegation (init, seeds = [b"delegation", vault])`.
  - `system_program: System`.
- **Behaviour**:
  - Verifies `delegate == vault.ephemeral_wallet`.
  - Writes `VaultDelegation` with `approved_at` = current time, `revoked_at = None`.
  - Emits `DelegateApproved` event.

### auto_deposit_for_trade
```rust
pub fn auto_deposit_for_trade(
    ctx: Context<AutoDeposit>,
    trade_fee_estimate: u64,
) -> Result<()>
```
- **Accounts**:
  - `vault: EphemeralVault (mut, has_one = parent_wallet)`.
  - `parent: Signer` – lamports are transferred from this account.
  - `system_program: System`.
- **Behaviour**:
  - Confirms vault is active and not expired.
  - Ensures `total_deposited + trade_fee_estimate <= max_deposit`.
  - CPI to `SystemProgram::transfer(parent -> vault)` for `trade_fee_estimate` lamports.
  - Updates `total_deposited` and emits `AutoDeposit` event.

### execute_trade
```rust
pub fn execute_trade(
    ctx: Context<ExecuteTrade>,
    fee_paid: u64,
) -> Result<()>
```
- **Accounts**:
  - `vault: EphemeralVault (mut, has_one = parent_wallet)`.
  - `ephemeral: signer` – must match `VaultDelegation.delegate`.
  - `delegation: VaultDelegation (mut, seeds = [b"delegation", vault])`.
  - `parent_wallet: UncheckedAccount` – for `has_one` checks.
- **Behaviour**:
  - Checks vault is active and not expired.
  - Confirms `delegation.vault == vault.key()`, `delegation.revoked_at.is_none()` and `delegation.delegate == ephemeral.key()`.
  - Increments `total_spent` by `fee_paid`, requiring that `total_spent <= total_deposited`.
  - Emits `TradeExecuted` event.

### revoke_access
```rust
pub fn revoke_access(ctx: Context<RevokeAccess>) -> Result<()>
```
- **Accounts**:
  - `vault: EphemeralVault (mut, has_one = parent_wallet)`.
  - `parent: Signer` – authority revoking access.
  - `delegation: VaultDelegation (mut, seeds = [b"delegation", vault])`.
  - `system_program: System`.
  - `parent_wallet: UncheckedAccount`.
- **Behaviour**:
  - Ensures vault is not already inactive, then sets `is_active = false`.
  - Sets `delegation.revoked_at = now`.
  - Returns remaining lamports (beyond rent-exempt minimum) from vault PDA to `parent` account.
  - Emits `AccessRevoked` event.

### cleanup_vault
```rust
pub fn cleanup_vault(ctx: Context<CleanupVault>) -> Result<()>
```
- **Accounts**:
  - `vault: EphemeralVault (mut, has_one = parent_wallet, close = parent)`.
  - `parent: mut` – receives final balance and rent.
  - `cleaner: Signer` – caller rewarded for cleanup.
  - `parent_wallet: UncheckedAccount`.
- **Behaviour**:
  - Requires `Clock::now() >= session_expiry`.
  - Marks vault inactive if still active.
  - Calculates lamports above rent-minimum and splits them into:
    - `reward` for `cleaner` (capped by `MAX_CLEANUP_REWARD_LAMPORTS`).
    - Remainder back to `parent`.
  - Emits `VaultCleaned` event.
  - Relies on Anchor `close = parent` attribute to reclaim rent to `parent` after instruction completes.

## Security Considerations
- All time checks use `Clock::get()` and compare `unix_timestamp` to `session_expiry`.
- `has_one` constraints ensure that only the configured `parent_wallet` can operate on a given vault.
- Delegation cannot be redirected to arbitrary wallets because `approve_delegate` enforces `delegate == vault.ephemeral_wallet`.
- Over-deposit is prevented via per-vault `max_deposit`.
- Funds can always be returned to parent either directly via `revoke_access` or indirectly after expiry via `cleanup_vault` called by any user.

## Limitations and Extensions
- The demo program does not integrate a real dark pool DEX via CPI; `execute_trade` is structured to support that integration.
- Token-based margin (e.g., USDC/USDT SPL tokens) can be added by extending `EphemeralVault` with token account PDAs and adding CPI calls to the SPL Token program.
- Multi-sig parent wallets and per-order spending limits can be supported via additional account metadata and checks in `create_vault` and `execute_trade`.
