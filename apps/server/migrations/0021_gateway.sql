-- The proxy is the gateway: its accounts' ids, names and folders say so.
UPDATE threads SET agent_account = 'gateway' || substr(agent_account, 6) WHERE agent_account LIKE 'proxy-%';
UPDATE usage SET agent_account = 'gateway' || substr(agent_account, 6) WHERE agent_account LIKE 'proxy-%';
UPDATE thread_sessions SET account = 'gateway' || substr(account, 6) WHERE account LIKE 'proxy-%';
UPDATE thread_sessions SET sessions_folder = replace(sessions_folder, '/proxy/proxy-', '/gateway/gateway-');
UPDATE settings SET value = replace(replace(replace(replace(replace(value,
    '"proxy-claude', '"gateway-claude'), '"proxy-codex', '"gateway-codex'), '/proxy/proxy-', '/gateway/gateway-'),
    '"proxy":', '"gateway":'), '"name":"Proxy"', '"name":"Gateway"')
WHERE name = 'agent_accounts';
