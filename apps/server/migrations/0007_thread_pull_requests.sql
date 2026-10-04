-- The pull request that was opened for a thread, as JSON, with what GitHub last said of it.
ALTER TABLE threads ADD COLUMN pull_request TEXT;
