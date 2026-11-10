-- Logs every project again, now with the repository it was cloned from, so that the account's other
-- servers' copies of it say so too.
UPDATE projects SET path = path;
