-- Each account's limit resets, rather than how many it has.
ALTER TABLE accounts ADD COLUMN resets TEXT NOT NULL DEFAULT '[]';
ALTER TABLE accounts DROP COLUMN reset_credits;
