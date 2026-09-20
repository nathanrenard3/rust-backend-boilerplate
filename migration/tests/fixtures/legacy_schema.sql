-- Snapshot of the tables previously created by the boilerplate's schema-sync.
CREATE TABLE users (
    id uuid NOT NULL PRIMARY KEY,
    email varchar NOT NULL UNIQUE,
    password_hash text NOT NULL,
    created_at bigint NOT NULL
);
CREATE TABLE auth_sessions (
    id varchar NOT NULL PRIMARY KEY,
    data json NOT NULL,
    expires_at bigint NOT NULL
);
CREATE INDEX "idx-auth_sessions-expires_at" ON auth_sessions (expires_at);
INSERT INTO users VALUES ('00000000-0000-0000-0000-000000000001', 'existing@example.com', 'existing-hash', 1);
INSERT INTO auth_sessions VALUES ('existing-session', '{"existing":true}', 9999999999);
