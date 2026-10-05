-- The thread's agent is told what happens on its pull request.
ALTER TABLE threads ADD COLUMN watching INTEGER NOT NULL DEFAULT 0;
