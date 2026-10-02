-- The image shown for a project: a file on the host, found in the project's folder or, with
-- `icon_chosen`, picked by the user.
ALTER TABLE projects ADD COLUMN icon TEXT;
ALTER TABLE projects ADD COLUMN icon_chosen INTEGER NOT NULL DEFAULT 0;
