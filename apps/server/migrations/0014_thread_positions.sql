-- Where the sidebar lists a thread among the active ones, the highest first: until now when it
-- was created or last came back from done, which is what the user can now change by moving it.
ALTER TABLE threads ADD COLUMN position REAL NOT NULL DEFAULT 0;
UPDATE threads SET position = MAX(created_at, COALESCE(undone_at, 0));
ALTER TABLE threads DROP COLUMN undone_at;
