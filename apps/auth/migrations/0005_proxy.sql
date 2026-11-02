-- The account's proxy, which serves its agents' logins to every server, is a device too.
ALTER TABLE devices DROP CONSTRAINT devices_kind_check;
ALTER TABLE devices ADD CONSTRAINT devices_kind_check CHECK (kind IN ('client', 'server', 'proxy'));
