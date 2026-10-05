-- What an agent spent on a model, each time it said so. Kept when its thread is deleted.
CREATE TABLE usage (
    at REAL NOT NULL,
    thread_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    agent TEXT NOT NULL,
    model TEXT NOT NULL,
    input INTEGER NOT NULL,
    cache_read INTEGER NOT NULL,
    cache_write INTEGER NOT NULL,
    output INTEGER NOT NULL,
    -- What the agent says it cost at the API's prices, when it says.
    cost_usd REAL
);

CREATE INDEX usage_by_time ON usage(at);

-- Claude Code counts from the start of a session: what it last said, to tell what was added.
CREATE TABLE usage_totals (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL,
    model TEXT NOT NULL,
    input INTEGER NOT NULL,
    cache_read INTEGER NOT NULL,
    cache_write INTEGER NOT NULL,
    output INTEGER NOT NULL,
    cost_usd REAL NOT NULL,
    PRIMARY KEY (thread_id, session_id, model)
);

-- A session that began before its usage was kept has totals nobody knows the start of: the
-- first ones it reports are only remembered.
ALTER TABLE threads ADD COLUMN usage_known INTEGER NOT NULL DEFAULT 1;
UPDATE threads SET usage_known = 0 WHERE session_id IS NOT NULL;
