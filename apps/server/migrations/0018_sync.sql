-- Every server keeps a copy of the account's other servers' threads, which the clients carry
-- between them. Each write to a project, a thread, an item or the usage takes the next number of
-- the server's change log, so that another server can be given what changed after the last one it
-- has. `log_id` names this log: a database restored from a backup starts another one.
CREATE TABLE sync (
    change INTEGER NOT NULL,
    log_id TEXT NOT NULL
);
INSERT INTO sync (change, log_id) VALUES (0, lower(hex(randomblob(16))));

ALTER TABLE projects ADD COLUMN change INTEGER NOT NULL DEFAULT 0;
ALTER TABLE threads ADD COLUMN change INTEGER NOT NULL DEFAULT 0;
ALTER TABLE items ADD COLUMN change INTEGER NOT NULL DEFAULT 0;
ALTER TABLE usage ADD COLUMN change INTEGER NOT NULL DEFAULT 0;

UPDATE projects SET change = numbered.n FROM (SELECT rowid AS r, ROW_NUMBER() OVER (ORDER BY rowid) AS n FROM projects) AS numbered
    WHERE projects.rowid = numbered.r;
UPDATE sync SET change = (SELECT COUNT(*) FROM projects);
UPDATE threads SET change = numbered.n + (SELECT change FROM sync)
    FROM (SELECT rowid AS r, ROW_NUMBER() OVER (ORDER BY rowid) AS n FROM threads) AS numbered
    WHERE threads.rowid = numbered.r;
UPDATE sync SET change = change + (SELECT COUNT(*) FROM threads);
UPDATE items SET change = numbered.n + (SELECT change FROM sync)
    FROM (SELECT rowid AS r, ROW_NUMBER() OVER (ORDER BY thread_id, seq) AS n FROM items) AS numbered
    WHERE items.rowid = numbered.r;
UPDATE sync SET change = change + (SELECT COUNT(*) FROM items);
UPDATE usage SET change = numbered.n + (SELECT change FROM sync)
    FROM (SELECT rowid AS r, ROW_NUMBER() OVER (ORDER BY rowid) AS n FROM usage) AS numbered
    WHERE usage.rowid = numbered.r;
UPDATE sync SET change = change + (SELECT COUNT(*) FROM usage);

CREATE INDEX projects_by_change ON projects(change);
CREATE INDEX threads_by_change ON threads(change);
CREATE INDEX items_by_change ON items(change);
CREATE INDEX usage_by_change ON usage(change);

-- What was deleted, so that the copies drop it too.
CREATE TABLE tombstones (
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    change INTEGER NOT NULL,
    PRIMARY KEY (kind, id)
);
CREATE INDEX tombstones_by_change ON tombstones(change);

-- A trigger's own update changes `change`, which the `WHEN` passes over.
CREATE TRIGGER projects_inserted AFTER INSERT ON projects BEGIN
    UPDATE sync SET change = change + 1;
    UPDATE projects SET change = (SELECT change FROM sync) WHERE rowid = NEW.rowid;
END;
CREATE TRIGGER projects_updated AFTER UPDATE ON projects WHEN NEW.change = OLD.change BEGIN
    UPDATE sync SET change = change + 1;
    UPDATE projects SET change = (SELECT change FROM sync) WHERE rowid = NEW.rowid;
END;
CREATE TRIGGER projects_deleted AFTER DELETE ON projects BEGIN
    UPDATE sync SET change = change + 1;
    INSERT OR REPLACE INTO tombstones (kind, id, change) VALUES ('project', OLD.id, (SELECT change FROM sync));
END;

CREATE TRIGGER threads_inserted AFTER INSERT ON threads BEGIN
    UPDATE sync SET change = change + 1;
    UPDATE threads SET change = (SELECT change FROM sync) WHERE rowid = NEW.rowid;
END;
CREATE TRIGGER threads_updated AFTER UPDATE ON threads WHEN NEW.change = OLD.change BEGIN
    UPDATE sync SET change = change + 1;
    UPDATE threads SET change = (SELECT change FROM sync) WHERE rowid = NEW.rowid;
END;
CREATE TRIGGER threads_deleted AFTER DELETE ON threads BEGIN
    UPDATE sync SET change = change + 1;
    INSERT OR REPLACE INTO tombstones (kind, id, change) VALUES ('thread', OLD.id, (SELECT change FROM sync));
END;

CREATE TRIGGER items_inserted AFTER INSERT ON items BEGIN
    UPDATE sync SET change = change + 1;
    UPDATE items SET change = (SELECT change FROM sync) WHERE rowid = NEW.rowid;
END;
CREATE TRIGGER items_updated AFTER UPDATE ON items WHEN NEW.change = OLD.change BEGIN
    UPDATE sync SET change = change + 1;
    UPDATE items SET change = (SELECT change FROM sync) WHERE rowid = NEW.rowid;
END;

CREATE TRIGGER usage_inserted AFTER INSERT ON usage BEGIN
    UPDATE sync SET change = change + 1;
    UPDATE usage SET change = (SELECT change FROM sync) WHERE rowid = NEW.rowid;
END;

-- The copies of other servers' threads. Each row keeps the number its origin's log gave it, so
-- that a copy can be passed on to a third server. `origins` says how far each copy goes.
CREATE TABLE origins (
    id TEXT PRIMARY KEY,
    log_id TEXT NOT NULL,
    change INTEGER NOT NULL,
    name TEXT NOT NULL,
    platform TEXT NOT NULL
);

CREATE TABLE copied_projects (
    origin TEXT NOT NULL,
    id TEXT NOT NULL,
    change INTEGER NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY (origin, id)
);
CREATE INDEX copied_projects_by_change ON copied_projects(origin, change);

CREATE TABLE copied_threads (
    origin TEXT NOT NULL,
    id TEXT NOT NULL,
    change INTEGER NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY (origin, id)
);
CREATE INDEX copied_threads_by_change ON copied_threads(origin, change);

CREATE TABLE copied_items (
    origin TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    change INTEGER NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY (origin, thread_id, id)
);
CREATE INDEX copied_items_by_change ON copied_items(origin, change);
CREATE INDEX copied_items_by_seq ON copied_items(origin, thread_id, seq);

CREATE TABLE copied_usage (
    origin TEXT NOT NULL,
    change INTEGER NOT NULL,
    at REAL NOT NULL,
    thread_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    agent TEXT NOT NULL,
    agent_account TEXT NOT NULL,
    model TEXT NOT NULL,
    input INTEGER NOT NULL,
    cache_read INTEGER NOT NULL,
    cache_write INTEGER NOT NULL,
    output INTEGER NOT NULL,
    cost_usd REAL,
    purpose TEXT NOT NULL,
    PRIMARY KEY (origin, change)
);
CREATE INDEX copied_usage_by_time ON copied_usage(origin, at);

CREATE TABLE copied_tombstones (
    origin TEXT NOT NULL,
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    change INTEGER NOT NULL,
    PRIMARY KEY (origin, kind, id)
);
CREATE INDEX copied_tombstones_by_change ON copied_tombstones(origin, change);
