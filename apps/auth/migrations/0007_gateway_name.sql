-- A gateway linked while it was the proxy kept the proxy's name.
UPDATE devices SET name = 'Gateway' WHERE kind = 'gateway' AND name = 'Proxy';
