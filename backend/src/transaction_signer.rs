use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use ring::aead;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};

pub fn encrypt_keypair(keypair: &Keypair, kek: &str) -> Result<String> {
    let serialized = keypair.to_bytes();
    let kek_bytes = kek.as_bytes();

    let salt = b"evs-key-salt";
    let mut key = [0u8; 32];
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA256,
        std::num::NonZeroU32::new(100_000).unwrap(),
        salt,
        kek_bytes,
        &mut key,
    );

    let unbound_key = aead::UnboundKey::new(&aead::AES_256_GCM, &key).context("invalid aead key")?;
    let nonce = aead::Nonce::assume_unique_for_key([0u8; 12]);
    let mut sealing_key = aead::LessSafeKey::new(unbound_key);
    let mut in_out = serialized.to_vec();
    in_out.extend_from_slice(&[0u8; aead::AES_256_GCM.tag_len()]);
    sealing_key
        .seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut in_out)
        .context("failed to encrypt keypair")?;

    Ok(general_purpose::STANDARD_NO_PAD.encode(in_out))
}

pub fn decrypt_keypair(ciphertext_b64: &str, kek: &str) -> Result<Keypair> {
    let mut ciphertext = general_purpose::STANDARD_NO_PAD
        .decode(ciphertext_b64)
        .context("invalid base64")?;

    let kek_bytes = kek.as_bytes();
    let salt = b"evs-key-salt";
    let mut key = [0u8; 32];
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA256,
        std::num::NonZeroU32::new(100_000).unwrap(),
        salt,
        kek_bytes,
        &mut key,
    );

    let unbound_key = aead::UnboundKey::new(&aead::AES_256_GCM, &key).context("invalid aead key")?;
    let nonce = aead::Nonce::assume_unique_for_key([0u8; 12]);
    let mut opening_key = aead::LessSafeKey::new(unbound_key);
    let plaintext = opening_key
        .open_in_place(nonce, aead::Aad::empty(), &mut ciphertext)
        .context("failed to decrypt keypair")?;

    let kp = Keypair::from_bytes(plaintext).context("invalid keypair bytes")?;
    Ok(kp)
}

pub struct TransactionSigner {
    rpc: RpcClient,
}

impl TransactionSigner {
    pub fn new(rpc_url: &str) -> Self {
        let rpc = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());
        Self { rpc }
    }

    pub async fn send_and_confirm(&self, tx: &Transaction) -> Result<Signature> {
        let sig = self.rpc.send_and_confirm_transaction(tx)?;
        Ok(sig)
    }
}
