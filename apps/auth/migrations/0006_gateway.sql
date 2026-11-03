-- The account's proxy is its gateway.
ALTER TABLE devices DROP CONSTRAINT devices_kind_check;
UPDATE devices SET kind = 'gateway' WHERE kind = 'proxy';
ALTER TABLE devices ADD CONSTRAINT devices_kind_check CHECK (kind IN ('client', 'server', 'gateway'));
