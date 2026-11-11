-- The stretches of time the user spent in a client, focused on it and using it, on a project of
-- this server or on none. They go into the change log, as the rest of the activity does.
CREATE TABLE engaged (
    device TEXT NOT NULL,
    project_id TEXT NOT NULL,
    started_at REAL NOT NULL,
    ended_at REAL NOT NULL,
    change INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (device, started_at)
);
CREATE INDEX engaged_by_time ON engaged(started_at);
CREATE INDEX engaged_by_change ON engaged(change);

CREATE TRIGGER engaged_inserted AFTER INSERT ON engaged BEGIN
    UPDATE sync SET change = change + 1;
    UPDATE engaged SET change = (SELECT change FROM sync) WHERE rowid = NEW.rowid;
END;

CREATE TABLE copied_engaged (
    origin TEXT NOT NULL,
    change INTEGER NOT NULL,
    device TEXT NOT NULL,
    project_id TEXT NOT NULL,
    started_at REAL NOT NULL,
    ended_at REAL NOT NULL,
    PRIMARY KEY (origin, device, started_at)
);
CREATE INDEX copied_engaged_by_time ON copied_engaged(origin, started_at);
