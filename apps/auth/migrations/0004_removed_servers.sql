-- A server removed from an account, kept so that its threads, which the account's other servers
-- hold copies of, still show its name. A server deleted completely has its copies dropped.
CREATE TABLE removed_servers (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    public_key TEXT NOT NULL,
    name TEXT NOT NULL,
    platform TEXT NOT NULL,
    removed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    PRIMARY KEY (user_id, public_key)
);
