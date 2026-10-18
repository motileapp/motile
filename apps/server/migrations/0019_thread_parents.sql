-- A thread another thread's agent started for a task: the thread and the tool call that started
-- it, and whether its agent is to be told when the turn it was given ends.
ALTER TABLE threads ADD COLUMN parent_thread TEXT;
ALTER TABLE threads ADD COLUMN parent_tool TEXT;
ALTER TABLE threads ADD COLUMN reports INTEGER NOT NULL DEFAULT 0;
