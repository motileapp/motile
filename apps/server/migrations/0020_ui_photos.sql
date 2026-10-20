-- The photos a program's query found (`Media` as JSON), so that it is looked up once.
CREATE TABLE ui_photos (
    query TEXT PRIMARY KEY,
    media TEXT NOT NULL,
    kept_at INTEGER NOT NULL DEFAULT (unixepoch())
);
