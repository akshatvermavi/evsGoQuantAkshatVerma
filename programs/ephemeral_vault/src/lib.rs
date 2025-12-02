use anchor_lang::prelude::*;
use anchor_lang::solana_program::system_instruction;
use anchor_lang::solana_program::program::invoke;

declare_id!("EpheVau1t1111111111111111111111111111111111");

#[program]
pub mod ephemeral_vault {
    use super::*;

    pub fn create_vault(
        ctx: Context<CreateVault>,
        session_duration: i64,
        max_deposit: u64,
        ephemeral_wallet: Pubkey,
    ) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        let clock = Clock::get()?;

        vault.parent_wallet = ctx.accounts.parent.key();
        vault.ephemeral_wallet = ephemeral_wallet;
        vault.session_start = clock.unix_timestamp;
        vault.session_expiry = clock
            .unix_timestamp
            .checked_add(session_duration)
            .ok_or(EphemeralVaultError::MathOverflow)?;
        vault.is_active = true;
        vault.total_deposited = 0;
        vault.total_spent = 0;
        vault.max_deposit = max_deposit;
        vault.bump = *ctx.bumps.get("vault").unwrap();

        emit!(VaultCreated {
            parent: ctx.accounts.parent.key(),
            vault: vault.key(),
            ephemeral_wallet,
            max_deposit,
            session_start: vault.session_start,
            session_expiry: vault.session_expiry,
        });

        Ok(())
    }

    pub fn approve_delegate(ctx: Context<ApproveDelegate>, delegate: Pubkey) -> Result<()> {
        let vault = &ctx.accounts.vault;

        require_keys_eq!(
            delegate,
            vault.ephemeral_wallet,
            EphemeralVaultError::InvalidDelegate
        );

        let clock = Clock::get()?;
        let delegation = &mut ctx.accounts.delegation;
        delegation.vault = vault.key();
        delegation.delegate = delegate;
        delegation.approved_at = clock.unix_timestamp;
        delegation.revoked_at = None;
        delegation.bump = *ctx.bumps.get("delegation").unwrap();

        emit!(DelegateApproved {
            vault: vault.key(),
            delegate,
            approved_at: clock.unix_timestamp,
        });

        Ok(())
    }

    pub fn auto_deposit_for_trade(
        ctx: Context<AutoDeposit>,
        trade_fee_estimate: u64,
    ) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        let parent = &ctx.accounts.parent;
        let system_program = &ctx.accounts.system_program;

        ensure_vault_active_and_not_expired(vault)?;

        let new_total = vault
            .total_deposited
            .checked_add(trade_fee_estimate)
            .ok_or(EphemeralVaultError::MathOverflow)?;
        require!(
            new_total <= vault.max_deposit,
            EphemeralVaultError::OverDeposit
        );

        let ix = system_instruction::transfer(&parent.key(), &vault.key(), trade_fee_estimate);
        invoke(
            &ix,
            &[parent.to_account_info(), vault.to_account_info(), system_program.to_account_info()],
        )?;

        vault.total_deposited = new_total;

        emit!(AutoDeposit {
            vault: vault.key(),
            amount: trade_fee_estimate,
            total_deposited: vault.total_deposited,
        });

        Ok(())
    }

    pub fn execute_trade(
        ctx: Context<ExecuteTrade>,
        fee_paid: u64,
    ) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        let delegation = &ctx.accounts.delegation;

        ensure_vault_active_and_not_expired(vault)?;

        // Ensure delegation is valid and not revoked.
        require_keys_eq!(
            delegation.vault,
            vault.key(),
            EphemeralVaultError::InvalidDelegationAccount
        );
        require!(
            delegation.revoked_at.is_none(),
            EphemeralVaultError::DelegationRevoked
        );
        require_keys_eq!(
            delegation.delegate,
            ctx.accounts.ephemeral.key(),
            EphemeralVaultError::InvalidDelegate
        );

        // In a full implementation, this is where CPI(s) to the dark pool DEX program
        // would be invoked using the vault funds and ephemeral wallet authority.

        let new_spent = vault
            .total_spent
            .checked_add(fee_paid)
            .ok_or(EphemeralVaultError::MathOverflow)?;
        require!(
            new_spent <= vault.total_deposited,
            EphemeralVaultError::InsufficientVaultBalance
        );
        vault.total_spent = new_spent;

        emit!(TradeExecuted {
            vault: vault.key(),
            delegate: ctx.accounts.ephemeral.key(),
            fee_paid,
            total_spent: vault.total_spent,
        });

        Ok(())
    }

    pub fn revoke_access(ctx: Context<RevokeAccess>) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        let delegation = &mut ctx.accounts.delegation;
        let parent = &ctx.accounts.parent;
        let system_program = &ctx.accounts.system_program;

        // Parent is signer via context constraint; mark inactive.
        ensure_vault_not_already_inactive(vault)?;
        vault.is_active = false;

        let clock = Clock::get()?;
        delegation.revoked_at = Some(clock.unix_timestamp);

        // Return remaining lamports (minus rent-exempt minimum) to parent.
        let vault_info = vault.to_account_info();
        let parent_info = parent.to_account_info();
        let min_balance = Rent::get()?.minimum_balance(vault_info.data_len());
        let current_balance = **vault_info.lamports.borrow();
        if current_balance > min_balance {
            let amount = current_balance
                .checked_sub(min_balance)
                .ok_or(EphemeralVaultError::MathOverflow)?;
            **vault_info.try_borrow_mut_lamports()? -= amount;
            **parent_info.try_borrow_mut_lamports()? += amount;
        }

        emit!(AccessRevoked {
            vault: vault.key(),
            parent: parent.key(),
            revoked_at: clock.unix_timestamp,
        });

        Ok(())
    }

    pub fn cleanup_vault(ctx: Context<CleanupVault>) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        let cleaner = &ctx.accounts.cleaner;
        let parent = &ctx.accounts.parent;

        let clock = Clock::get()?;
        require!(
            clock.unix_timestamp >= vault.session_expiry,
            EphemeralVaultError::SessionNotExpired
        );

        // If still active, mark inactive.
        if vault.is_active {
            vault.is_active = false;
        }

        // Pay small reward to cleaner from vault lamports, bounded by constant.
        let vault_info = vault.to_account_info();
        let parent_info = parent.to_account_info();
        let cleaner_info = cleaner.to_account_info();
        let min_balance = Rent::get()?.minimum_balance(vault_info.data_len());
        let current_balance = **vault_info.lamports.borrow();

        const MAX_CLEANUP_REWARD_LAMPORTS: u64 = 10_000; // small fixed reward cap

        if current_balance > min_balance {
            let available = current_balance
                .checked_sub(min_balance)
                .ok_or(EphemeralVaultError::MathOverflow)?;
            let reward = available.min(MAX_CLEANUP_REWARD_LAMPORTS);
            let to_parent = available
                .checked_sub(reward)
                .ok_or(EphemeralVaultError::MathOverflow)?;

            **vault_info.try_borrow_mut_lamports()? -= available;
            **cleaner_info.try_borrow_mut_lamports()? += reward;
            **parent_info.try_borrow_mut_lamports()? += to_parent;

            emit!(VaultCleaned {
                vault: vault.key(),
                parent: parent.key(),
                cleaner: cleaner.key(),
                reward,
            });
        }

        Ok(())
    }
}

fn ensure_vault_active_and_not_expired(vault: &EphemeralVault) -> Result<()> {
    require!(vault.is_active, EphemeralVaultError::VaultInactive);
    let clock = Clock::get()?;
    require!(
        clock.unix_timestamp <= vault.session_expiry,
        EphemeralVaultError::SessionExpired
    );
    Ok(())
}

fn ensure_vault_not_already_inactive(vault: &EphemeralVault) -> Result<()> {
    require!(vault.is_active, EphemeralVaultError::VaultInactive);
    Ok(())
}

#[derive(Accounts)]
pub struct CreateVault<'info> {
    #[account(mut)]
    pub parent: Signer<'info>,

    /// CHECK: Ephemeral wallet is an off-chain keypair; we only store its pubkey.
    pub ephemeral_wallet: UncheckedAccount<'info>,

    #[account(
        init,
        payer = parent,
        space = 8 + EphemeralVault::LEN,
        seeds = [b"vault", parent.key().as_ref(), ephemeral_wallet.key().as_ref()],
        bump,
    )]
    pub vault: Account<'info, EphemeralVault>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ApproveDelegate<'info> {
    #[account(mut, has_one = parent_wallet)]
    pub vault: Account<'info, EphemeralVault>,

    #[account(mut)]
    pub parent: Signer<'info>,

    #[account(
        init,
        payer = parent,
        space = 8 + VaultDelegation::LEN,
        seeds = [b"delegation", vault.key().as_ref()],
        bump,
    )]
    pub delegation: Account<'info, VaultDelegation>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AutoDeposit<'info> {
    #[account(mut, has_one = parent_wallet)]
    pub vault: Account<'info, EphemeralVault>,

    #[account(mut)]
    pub parent: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ExecuteTrade<'info> {
    #[account(mut, has_one = parent_wallet)]
    pub vault: Account<'info, EphemeralVault>,

    /// CHECK: Ephemeral wallet must sign to execute trades.
    #[account(signer)]
    pub ephemeral: AccountInfo<'info>,

    #[account(
        mut,
        seeds = [b"delegation", vault.key().as_ref()],
        bump = delegation.bump,
    )]
    pub delegation: Account<'info, VaultDelegation>,

    /// Parent wallet is stored for has_one checks but does not need to sign here.
    /// CHECK: Only used for has_one relationship; actual authority for executing trades is the ephemeral wallet.
    pub parent_wallet: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct RevokeAccess<'info> {
    #[account(mut, has_one = parent_wallet)]
    pub vault: Account<'info, EphemeralVault>,

    #[account(mut, signer)]
    pub parent: AccountInfo<'info>,

    #[account(
        mut,
        seeds = [b"delegation", vault.key().as_ref()],
        bump = delegation.bump,
    )]
    pub delegation: Account<'info, VaultDelegation>,

    pub system_program: Program<'info, System>,

    /// CHECK: Only used for has_one constraint.
    pub parent_wallet: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct CleanupVault<'info> {
    #[account(mut, has_one = parent_wallet, close = parent)]
    pub vault: Account<'info, EphemeralVault>,

    /// CHECK: Parent wallet receives reclaimed rent and remaining funds.
    #[account(mut)]
    pub parent: AccountInfo<'info>,

    /// CHECK: Anyone can trigger cleanup and receive a small reward.
    #[account(mut, signer)]
    pub cleaner: AccountInfo<'info>,

    /// CHECK: Only used for has_one constraint.
    pub parent_wallet: UncheckedAccount<'info>,
}

#[account]
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

impl EphemeralVault {
    pub const LEN: usize = 32 + 32 + 8 + 8 + 1 + 8 + 8 + 8 + 1;
}

#[account]
pub struct VaultDelegation {
    pub vault: Pubkey,
    pub delegate: Pubkey,
    pub approved_at: i64,
    pub revoked_at: Option<i64>,
    pub bump: u8,
}

impl VaultDelegation {
    // 32 (vault) + 32 (delegate) + 8 (approved_at) + 1 + 8 (Option<i64>) + 1 (bump)
    pub const LEN: usize = 32 + 32 + 8 + 1 + 8 + 1;
}

#[event]
pub struct VaultCreated {
    pub parent: Pubkey,
    pub vault: Pubkey,
    pub ephemeral_wallet: Pubkey,
    pub max_deposit: u64,
    pub session_start: i64,
    pub session_expiry: i64,
}

#[event]
pub struct DelegateApproved {
    pub vault: Pubkey,
    pub delegate: Pubkey,
    pub approved_at: i64,
}

#[event]
pub struct AutoDeposit {
    pub vault: Pubkey,
    pub amount: u64,
    pub total_deposited: u64,
}

#[event]
pub struct TradeExecuted {
    pub vault: Pubkey,
    pub delegate: Pubkey,
    pub fee_paid: u64,
    pub total_spent: u64,
}

#[event]
pub struct AccessRevoked {
    pub vault: Pubkey,
    pub parent: Pubkey,
    pub revoked_at: i64,
}

#[event]
pub struct VaultCleaned {
    pub vault: Pubkey,
    pub parent: Pubkey,
    pub cleaner: Pubkey,
    pub reward: u64,
}

#[error_code]
pub enum EphemeralVaultError {
    #[msg("Math overflow")] 
    MathOverflow,
    #[msg("Vault session expired")] 
    SessionExpired,
    #[msg("Vault session not yet expired")] 
    SessionNotExpired,
    #[msg("Vault is inactive")] 
    VaultInactive,
    #[msg("Invalid delegate for this vault")] 
    InvalidDelegate,
    #[msg("Invalid delegation account for this vault")] 
    InvalidDelegationAccount,
    #[msg("Delegation has been revoked")] 
    DelegationRevoked,
    #[msg("Over-deposit attempt beyond approved max_deposit")] 
    OverDeposit,
    #[msg("Insufficient vault balance for requested fee")] 
    InsufficientVaultBalance,
}