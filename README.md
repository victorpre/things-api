# things-api

A local HTTP API for Things 3 tasks.

The API reads directly from the Things 3 SQLite database and opens that database connection in read-only mode. Write routes use Things' official URL scheme instead of writing to SQLite.

## Setup

Copy the example environment file and point it at your Things database:

```sh
cp .env.example .env
```

Edit `.env`:

```sh
THINGS_DB_PATH="/Users/you/Library/Group Containers/JLMPQHK86H.com.culturedcode.ThingsMac/ThingsData-XXXX/Things Database.thingsdatabase/main.sqlite"
THINGS_API_HOST="127.0.0.1"
THINGS_API_PORT="3000"
WHISPER_INFERENCE_URL="http://127.0.0.1:8080/inference"
```

Cultured Code documents the Things database location here:
https://culturedcode.com/things/support/articles/2982272/

## Run

```sh
cargo run
```

The server listens on `THINGS_API_HOST:THINGS_API_PORT`, defaulting to `127.0.0.1:3000`.

For a reTerminal Sticky or another device on the same network, set `THINGS_API_HOST` to `0.0.0.0` or to the Mac's LAN IP, then call the Mac's LAN IP from the device.

## Endpoints

```sh
curl http://127.0.0.1:3000/health
curl http://127.0.0.1:3000/tasks
curl http://127.0.0.1:3000/tasks/lists
curl http://127.0.0.1:3000/tasks/today
curl -X POST http://127.0.0.1:3000/tasks/from-audio \
  -F file="@/path/to/todo.wav"
```

`GET /tasks` supports these optional query parameters:

- `status`: `open`, `completed`, `canceled`, or a raw Things status code
- `trashed`: `true` or `false`
- `include_trashed`: `true` or `false`

`GET /tasks/today` mirrors Things' Today list query: open, untrashed todos scheduled for today or earlier, including due-date-only tasks due today or earlier.

`GET /tasks/lists` returns selected Things lists in one response. By default it returns Inbox and Today:

```json
{
  "lists": [
    { "id": "inbox", "title": "Inbox", "tasks": [] },
    { "id": "today", "title": "Today", "tasks": [] }
  ]
}
```

Use `selected` to choose and order lists:

```sh
curl "http://127.0.0.1:3000/tasks/lists?selected=today,inbox"
curl "http://127.0.0.1:3000/tasks/lists?selected=inbox"
```

Supported list ids are `inbox` and `today`. Inbox mirrors Things' Inbox query: open, untrashed, unscheduled, non-repeating todos without a project. Today matches `GET /tasks/today`.

`POST /tasks/from-audio` accepts multipart field `file`, forwards the WAV upload to whisper.cpp, normalizes the returned transcript, creates an Inbox task through `things:///add`, and returns `202 Accepted` after macOS accepts the URL launch:

```json
{
  "attributes": {
    "title": "Clean coffee machine"
  }
}
```

The Whisper request always sends these form fields:

- `temperature`: `0.0`
- `temperature_inc`: `0.2`
- `response_format`: `json`
- `prompt`: `Identify the task todo`
- `carry_initial_prompt`: `true`

Examples:

```sh
curl "http://127.0.0.1:3000/tasks?status=open"
curl "http://127.0.0.1:3000/tasks?include_trashed=true"
curl "http://127.0.0.1:3000/tasks/lists?selected=today,inbox"
curl "http://127.0.0.1:3000/tasks/today"
curl -X POST "http://127.0.0.1:3000/tasks/from-audio" \
  -F file="@/path/to/todo.wav"
```

Things must still run on the same Mac as `things-api`, because task creation uses the local Things URL scheme. The API launches that URL with `open -g` so Things should not be brought to the foreground, but macOS still needs to route the URL to the Things app.

## Development

```sh
cargo test
```
