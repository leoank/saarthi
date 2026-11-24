-- Each Claude token's default model (ADR-0079): what a session under it starts on when none
-- is picked, as `--model` takes it; empty for Claude Code's own default. A renewed token
-- keeps it: the model follows the account, not the token's value.
ALTER TABLE claude_tokens ADD COLUMN default_model TEXT NOT NULL DEFAULT '';
