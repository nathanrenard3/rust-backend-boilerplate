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
- Auth placeholder: `curl -i http://localhost:8000/auth` (HTTP 200 with body `ok`).
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

## Source layout

- `src/main.rs`: initializes the database pool and starts the HTTP server.
- `src/routes.rs`: maps URLs to handlers and shares the database pool.
- `src/settings.rs`: general endpoints (`/` and `/health`).
- `src/auth.rs`: authentication module, currently a placeholder endpoint.

Handlers that need the database receive the shared pool through
`State<DatabaseConnection>`, as shown in `settings::health`. They do not call
`Database::connect` again.
