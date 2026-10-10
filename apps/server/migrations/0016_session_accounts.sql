-- The account a session last took a turn with, for a handoff to name.
ALTER TABLE thread_sessions ADD COLUMN account TEXT;
