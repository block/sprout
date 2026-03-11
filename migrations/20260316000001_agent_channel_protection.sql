-- Migration: add agent channel protection fields to users table
ALTER TABLE users
  ADD COLUMN agent_owner_pubkey VARBINARY(32) NULL,
  ADD COLUMN channel_add_policy ENUM('anyone', 'owner_only', 'nobody')
    NOT NULL DEFAULT 'anyone',
  ADD CONSTRAINT fk_users_agent_owner
    FOREIGN KEY (agent_owner_pubkey) REFERENCES users(pubkey)
    ON DELETE SET NULL;
