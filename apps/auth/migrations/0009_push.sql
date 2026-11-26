-- Where a phone takes pushes: APNs or FCM. What is pushed is sealed to the phone's device key.
CREATE TABLE push_registrations (
    device TEXT PRIMARY KEY REFERENCES devices(public_key) ON DELETE CASCADE,
    -- `apns` or `fcm`; an APNs registration has the app's bundle identifier as its topic.
    service TEXT NOT NULL CHECK (service IN ('apns', 'fcm')),
    topic TEXT,
    sandbox BOOLEAN NOT NULL DEFAULT false,
    token TEXT NOT NULL,
    -- The `AlertChoices`, as JSON.
    choices TEXT NOT NULL,
    live BOOLEAN NOT NULL DEFAULT false,
    live_start_token TEXT,
    live_token TEXT,
    -- When a Live Activity was last started on the phone, so that it is started once.
    live_started_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- What each server's agents are doing, sealed for one phone: the latest a server sent.
CREATE TABLE push_work (
    device TEXT NOT NULL REFERENCES push_registrations(device) ON DELETE CASCADE,
    server TEXT NOT NULL REFERENCES devices(public_key) ON DELETE CASCADE,
    working INTEGER NOT NULL,
    waiting INTEGER NOT NULL,
    sealed TEXT NOT NULL,
    at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (device, server)
);
