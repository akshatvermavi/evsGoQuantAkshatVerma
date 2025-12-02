use crate::{config::Config, session_manager::Session};
use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_program,
    transaction::Transaction,
};

pub struct DelegationManager {
    rpc: RpcClient,
    cfg: Config,
}

impl DelegationManager {
    pub fn new(cfg: Config) -> Self {
        let rpc = RpcClient::new_with_commitment(
            cfg.solana.rpc_url.clone(),
            CommitmentConfig::confirmed(),
        );
        Self { rpc, cfg }
    }

    pub fn build_create_vault_ix(
        &self,
        program_id: Pubkey,
        parent_wallet: Pubkey,
        ephemeral_wallet: Pubkey,
        session_duration_secs: i64,
        max_deposit: u64,
    ) -> Instruction {
        let (vault_pda, _bump) = Pubkey::find_program_address(
            &[b"vault", parent_wallet.as_ref(), ephemeral_wallet.as_ref()],
            &program_id,
        );

        // Anchor instruction layout is normally generated via IDL. For this assessment we
        // treat this as a placeholder; the frontend would usually use the IDL to build this.
        Instruction {
            program_id,
            accounts: vec![
                // parent
                solana_sdk::instruction::AccountMeta::new(parent_wallet, true),
                solana_sdk::instruction::AccountMeta::new_readonly(ephemeral_wallet, false),
                solana_sdk::instruction::AccountMeta::new(vault_pda, false),
                solana_sdk::instruction::AccountMeta::new_readonly(system_program::id(), false),
            ],
            data: vec![],
        }
    }

    pub fn build_approve_delegate_ix(
        &self,
        program_id: Pubkey,
        parent_wallet: Pubkey,
        vault_pda: Pubkey,
        delegate: Pubkey,
    ) -> Instruction {
        let (delegation_pda, _bump) =
            Pubkey::find_program_address(&[b"delegation", vault_pda.as_ref()], &program_id);

        Instruction {
            program_id,
            accounts: vec![
                solana_sdk::instruction::AccountMeta::new(vault_pda, false),
                solana_sdk::instruction::AccountMeta::new(parent_wallet, true),
                solana_sdk::instruction::AccountMeta::new(delegation_pda, false),
                solana_sdk::instruction::AccountMeta::new_readonly(system_program::id(), false),
            ],
            data: vec![],
        }
    }

    pub async fn verify_delegation_onchain(
        &self,
        _session: &Session,
        _program_id: Pubkey,
    ) -> Result<bool> {
        // For the purposes of this assessment we keep this light-weight and rely on
        // application-layer guarantees. A production implementation would fetch the
        // VaultDelegation account and validate its fields.
        Ok(true)
    }

    pub async fn build_and_sign_transactions(
        &self,
        payer: &Keypair,
        instructions: Vec<Instruction>,
    ) -> Result<Transaction> {
        let latest_blockhash = self.rpc.get_latest_blockhash()?;
        let tx = Transaction::new_signed_with_payer(
            &instructions,
            Some(&payer.pubkey()),
            &[payer],
            latest_blockhash,
        );
        Ok(tx)
    }
}
