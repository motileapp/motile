-- Whether the sign-in was started by the web app, which gets the code at its own address and
-- opens a session with it instead of linking a device.
ALTER TABLE sign_ins ADD COLUMN web BOOLEAN NOT NULL DEFAULT false;

-- A browser signed in to the web app. Only the token's hash is kept.
CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);
