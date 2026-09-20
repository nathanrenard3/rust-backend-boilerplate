# Rust backend boilerplate

An Axum API and PostgreSQL database running together with Docker Compose.

## Getting started

With Docker running, copy the example configuration and adjust the credentials
and ports in `.env`:

```sh
cp .env.example .env
docker compose up --build -d
```

- Database health: `curl -i http://localhost:8000/health` (HTTP 200 when PostgreSQL
  responds, HTTP 503 otherwise).
- PostgreSQL from your machine: `localhost:5432`, using the credentials in `.env`.

Set the host ports with `API_PORT` and `POSTGRES_PORT` in `.env`.
Inside Docker, the API connects to `db:5432` using `DATABASE_URL`, built by
Compose from the PostgreSQL credentials. It waits for PostgreSQL to be ready
before starting.

```sh
docker compose logs -f api
docker compose down
```

`docker compose down` preserves the database in the `db_data` volume. PostgreSQL
credentials in `.env` initialize an empty database; changing them later does not
update users in an existing database.

After changing the Rust code, run `docker compose up --build -d` again.

## Authentication

Authentication uses `axum-login` with Argon2id password hashing and cookie sessions
stored in PostgreSQL. Register with `POST /auth/register`, log in with
`POST /auth/login`, retrieve the current user with `GET /auth/me`, and log out
with `POST /auth/logout`. Registration and login accept JSON containing `email`
and `password`; all POST requests require the `X-CSRF-Protection: 1` header.
Clients must retain and send the session cookie. Set `FRONTEND_ORIGIN` for your
browser client and enable `COOKIE_SECURE=true` when serving the API over HTTPS.

## Migrations

Versioned SeaORM migrations live in `migration/`. The API applies pending migrations
before starting; `seaql_migrations` records which ones have already run.
The initial migrations also accept the unchanged tables from the previous
schema-sync setup, preserving their data. They do not reconcile a modified schema.

```sh
docker compose exec api migrate status
docker compose exec api migrate up
```

To create a migration locally, run this from the project root:

```sh
(cd migration && cargo run -- generate add_username_to_users)
```

Fill in its `up` (apply) and `down` (undo) methods, then update the corresponding
entity in `src/`. Changing an entity alone no longer changes the database.
Add new migrations instead of editing ones that have already been applied.
Rebuild the API image to include them. For multiple API instances, run migrations
once before starting the instances.

`docker compose exec api migrate down -n 1` rolls back the last migration.
The initial migrations drop their tables on rollback, including all stored data.
