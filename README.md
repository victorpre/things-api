# things-api

A local, read-only HTTP API for Things 3 tasks.

The API reads directly from the Things 3 SQLite database and does not expose write routes. The database connection is opened with SQLite read-only mode.

## Setup

Copy the example environment file and point it at your Things database:

```sh
cp .env.example .env
```

Edit `.env`:

```sh
THINGS_DB_PATH="/Users/you/Library/Group Containers/JLMPQHK86H.com.culturedcode.ThingsMac/ThingsData-XXXX/Things Database.thingsdatabase/main.sqlite"
```

Cultured Code documents the Things database location here:
https://culturedcode.com/things/support/articles/2982272/

## Run

```sh
cargo run
```

The server listens on `127.0.0.1:3000`.

## Endpoints

```sh
curl http://127.0.0.1:3000/health
curl http://127.0.0.1:3000/tasks
curl http://127.0.0.1:3000/tasks/today
```

`GET /tasks` supports these optional query parameters:

- `status`: `open`, `completed`, `canceled`, or a raw Things status code
- `trashed`: `true` or `false`
- `include_trashed`: `true` or `false`

`GET /tasks/today` mirrors Things' Today list query: open, untrashed todos scheduled for today or earlier, including due-date-only tasks due today or earlier.

Examples:

```sh
curl "http://127.0.0.1:3000/tasks?status=open"
curl "http://127.0.0.1:3000/tasks?include_trashed=true"
curl "http://127.0.0.1:3000/tasks/today"
```

## Development

```sh
cargo test
```
