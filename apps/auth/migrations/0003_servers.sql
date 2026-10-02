-- A machine that runs agents is called a server, not a host.
ALTER TABLE devices DROP CONSTRAINT devices_kind_check;
UPDATE devices SET kind = 'server' WHERE kind = 'host';
ALTER TABLE devices ADD CONSTRAINT devices_kind_check CHECK (kind IN ('client', 'server'));
