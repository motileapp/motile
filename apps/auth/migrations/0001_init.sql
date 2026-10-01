CREATE TABLE users (
    id UUID PRIMARY KEY,
    -- Google's subject, or `dev:<email>` for accounts made by the dev login.
    subject TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL,
    name TEXT,
    picture TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_sign_in_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- A device is an ed25519 public key: an app (client) or a machine that runs agents (host).
CREATE TABLE devices (
    public_key TEXT PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('client', 'host')),
    name TEXT NOT NULL,
    platform TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX devices_by_user ON devices(user_id);

-- A sign-in an app started. `id` is the state sent to Google; `code_hash` is set once Google
-- has answered and is what the app exchanges, together with the secret behind `challenge`.
CREATE TABLE sign_ins (
    id TEXT PRIMARY KEY,
    challenge TEXT NOT NULL,
    app_state TEXT NOT NULL,
    google_verifier TEXT NOT NULL,
    nonce TEXT NOT NULL,
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    code_hash TEXT UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE enroll_tokens (
    token_hash TEXT PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- The host that used the token. It may use it again, so a failed install can be rerun.
    used_by TEXT,
    expires_at TIMESTAMPTZ NOT NULL
);
