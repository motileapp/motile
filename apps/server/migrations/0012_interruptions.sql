-- What the thread's agent was doing when the thread was last saved, which a restart cuts off, and
-- why the agent stopped before it finished, as JSON.
ALTER TABLE threads ADD COLUMN running INTEGER NOT NULL DEFAULT 0;
ALTER TABLE threads ADD COLUMN monitoring INTEGER NOT NULL DEFAULT 0;
ALTER TABLE threads ADD COLUMN interruption TEXT;
