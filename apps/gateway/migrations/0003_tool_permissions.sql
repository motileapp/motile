-- What agents may do with each kind of tool and each tool: allow, ask or block.
ALTER TABLE mcp_servers ADD COLUMN permissions TEXT NOT NULL DEFAULT '{}';
UPDATE mcp_servers SET permissions = json_object('tools', (SELECT json(json_group_object(value, 'block')) FROM json_each(off)));
ALTER TABLE mcp_servers DROP COLUMN off;
