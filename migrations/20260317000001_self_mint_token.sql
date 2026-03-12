-- Add created_by_self_mint column to api_tokens.
-- Tracks whether a token was minted via POST /api/tokens (self-service)
-- or via the sprout-admin CLI.
ALTER TABLE api_tokens
  ADD COLUMN created_by_self_mint TINYINT(1) NOT NULL DEFAULT 0
    COMMENT '1 = minted via POST /api/tokens, 0 = minted via sprout-admin CLI';
