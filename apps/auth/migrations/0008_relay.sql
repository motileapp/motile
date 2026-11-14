-- The account's own iroh relay, which its devices use in place of iroh's public ones.
ALTER TABLE users ADD COLUMN relay_url TEXT;
