-- Core tables for Ephemeral Vault System

CREATE TABLE IF NOT EXISTS sessions (
    id                  UUID PRIMARY KEY,
    parent_wallet       TEXT NOT NULL,
    ephemeral_wallet    TEXT NOT NULL,
    vault_pubkey        TEXT,
    status              TEXT NOT NULL,
    session_start       TIMESTAMPTZ NOT NULL,
    session_expiry      TIMESTAMPTZ NOT NULL,
    last_activity       TIMESTAMPTZ NOT NULL,
    max_deposit         BIGINT NOT NULL,
    total_deposited     BIGINT NOT NULL,
    total_spent         BIGINT NOT NULL,
    encrypted_ephemeral_key TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_parent_wallet ON sessions(parent_wallet);
CREATE INDEX IF NOT EXISTS idx_sessions_status ON sessions(status);

CREATE TABLE IF NOT EXISTS vault_transactions (
    id              UUID PRIMARY KEY,
    session_id      UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    vault_pubkey    TEXT NOT NULL,
    tx_signature    TEXT NOT NULL,
    tx_type         TEXT NOT NULL,
    amount          BIGINT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_vault_tx_session ON vault_transactions(session_id);

CREATE TABLE IF NOT EXISTS delegations (
    id              UUID PRIMARY KEY,
    session_id      UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    vault_pubkey    TEXT NOT NULL,
    delegate_pubkey TEXT NOT NULL,
    approved_at     TIMESTAMPTZ NOT NULL,
    revoked_at      TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS cleanup_events (
    id              UUID PRIMARY KEY,
    session_id      UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    vault_pubkey    TEXT NOT NULL,
    cleaner_pubkey  TEXT NOT NULL,
    reward_amount   BIGINT NOT NULL,
    executed_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS session_metrics (
    session_id          UUID PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    num_trades          BIGINT NOT NULL DEFAULT 0,
    total_fees_paid     BIGINT NOT NULL DEFAULT 0,
    last_updated        TIMESTAMPTZ NOT NULL DEFAULT now()
);
