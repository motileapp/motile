-- The messages the sidebar's search finds, by the item's rowid: trigrams find any part of a word,
-- whatever its case and accents. Apostrophes are kept straight, as a search makes them.

CREATE VIRTUAL TABLE item_search USING fts5(
    text, content = '', contentless_delete = 1, tokenize = 'trigram remove_diacritics 1'
);

CREATE TRIGGER item_search_insert AFTER INSERT ON items
WHEN json_extract(new.payload, '$.type') IN ('user', 'assistant') AND json_extract(new.payload, '$.parent') IS NULL
BEGIN
    INSERT INTO item_search (rowid, text)
    SELECT new.rowid, replace(replace(json_extract(new.payload, '$.text'), '’', ''''), '‘', '''')
    WHERE json_extract(new.payload, '$.text') IS NOT NULL;
END;

CREATE TRIGGER item_search_update AFTER UPDATE OF payload ON items
WHEN json_extract(old.payload, '$.text') IS NOT json_extract(new.payload, '$.text')
  OR json_extract(old.payload, '$.type') IS NOT json_extract(new.payload, '$.type')
  OR json_extract(old.payload, '$.parent') IS NOT json_extract(new.payload, '$.parent')
BEGIN
    DELETE FROM item_search WHERE rowid = old.rowid;
    INSERT INTO item_search (rowid, text)
    SELECT new.rowid, replace(replace(json_extract(new.payload, '$.text'), '’', ''''), '‘', '''')
    WHERE json_extract(new.payload, '$.type') IN ('user', 'assistant')
      AND json_extract(new.payload, '$.parent') IS NULL
      AND json_extract(new.payload, '$.text') IS NOT NULL;
END;

CREATE TRIGGER item_search_delete AFTER DELETE ON items
WHEN json_extract(old.payload, '$.type') IN ('user', 'assistant')
BEGIN
    DELETE FROM item_search WHERE rowid = old.rowid;
END;

INSERT INTO item_search (rowid, text)
SELECT rowid, replace(replace(json_extract(payload, '$.text'), '’', ''''), '‘', '''')
FROM items
WHERE json_valid(payload)
  AND json_extract(payload, '$.type') IN ('user', 'assistant')
  AND json_extract(payload, '$.parent') IS NULL
  AND json_extract(payload, '$.text') IS NOT NULL;

CREATE VIRTUAL TABLE copied_item_search USING fts5(
    text, content = '', contentless_delete = 1, tokenize = 'trigram remove_diacritics 1'
);

CREATE TRIGGER copied_item_search_insert AFTER INSERT ON copied_items
WHEN json_extract(new.payload, '$.type') IN ('user', 'assistant') AND json_extract(new.payload, '$.parent') IS NULL
BEGIN
    INSERT INTO copied_item_search (rowid, text)
    SELECT new.rowid, replace(replace(json_extract(new.payload, '$.text'), '’', ''''), '‘', '''')
    WHERE json_extract(new.payload, '$.text') IS NOT NULL;
END;

CREATE TRIGGER copied_item_search_update AFTER UPDATE OF payload ON copied_items
WHEN json_extract(old.payload, '$.text') IS NOT json_extract(new.payload, '$.text')
  OR json_extract(old.payload, '$.type') IS NOT json_extract(new.payload, '$.type')
  OR json_extract(old.payload, '$.parent') IS NOT json_extract(new.payload, '$.parent')
BEGIN
    DELETE FROM copied_item_search WHERE rowid = old.rowid;
    INSERT INTO copied_item_search (rowid, text)
    SELECT new.rowid, replace(replace(json_extract(new.payload, '$.text'), '’', ''''), '‘', '''')
    WHERE json_extract(new.payload, '$.type') IN ('user', 'assistant')
      AND json_extract(new.payload, '$.parent') IS NULL
      AND json_extract(new.payload, '$.text') IS NOT NULL;
END;

CREATE TRIGGER copied_item_search_delete AFTER DELETE ON copied_items
WHEN json_extract(old.payload, '$.type') IN ('user', 'assistant')
BEGIN
    DELETE FROM copied_item_search WHERE rowid = old.rowid;
END;

INSERT INTO copied_item_search (rowid, text)
SELECT rowid, replace(replace(json_extract(payload, '$.text'), '’', ''''), '‘', '''')
FROM copied_items
WHERE json_valid(payload)
  AND json_extract(payload, '$.type') IN ('user', 'assistant')
  AND json_extract(payload, '$.parent') IS NULL
  AND json_extract(payload, '$.text') IS NOT NULL;
