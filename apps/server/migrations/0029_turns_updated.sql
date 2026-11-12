-- A turn whose starter is read again from who sent its first message goes into the change log
-- again, and the copies take it in place of the one they have.
CREATE TRIGGER turns_updated AFTER UPDATE ON turns WHEN NEW.change = OLD.change BEGIN
    UPDATE sync SET change = change + 1;
    UPDATE turns SET change = (SELECT change FROM sync) WHERE rowid = NEW.rowid;
END;
