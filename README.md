# stress-node

A simple Node.js server that stresses a machine or a container on demand: CPU load, memory, disk space, slow responses and arbitrary HTTP status codes. Useful for testing autoscaling, resource limits, OOM kills, probes, timeouts and retries.

Source code and issues: https://github.com/altrdev/stress-node

> **Warning:** this server is meant to consume resources on purpose. Don't expose it to the public internet.

## Quick start

With Docker:

```
docker run -p 3000:3000 altrdev/stress-node:latest
```

Or on your local machine (Node.js 22 or later):

```
npm install
npm start
```

Then try it:

```
curl "http://127.0.0.1:3000/stress?number=40"
```

To restart the server automatically when you edit the code, use `npm run dev` instead of `npm start`.

## Endpoints

The server listens on port 3000.

| Endpoint        | Parameters         | Description                                                 |
|-----------------|--------------------|-------------------------------------------------------------|
| `/`             |                    | Health check, returns `Status: OK`                          |
| `/stress`       | `number`           | Runs a Fibonacci calculation to load a CPU                  |
| `/delay`        | `ms`               | Answers after a delay, to test timeouts and slow backends   |
| `/status/:code` | `code` (in the path) | Answers with the given HTTP status, to test probes and retries |
| `/memory`       | `mb`, `hold`       | Allocates memory and holds it, to test memory limits and OOM kills |
| `/disk`         | `mb`, `hold`       | Writes a file and keeps it, to test disk limits and volumes |

Every query parameter is optional. A value that is not a number falls back to its default; a number outside the allowed range gets a `400`. All the limits are configurable, see [Configuration](#configuration).

### `/stress`

Runs a recursive Fibonacci calculation in a worker process, so it keeps one core busy for the whole request.

| Parameter | Default | Allowed range      |
|-----------|---------|--------------------|
| `number`  | `45`    | `0..MAX_NUMBER`    |

The calculation time grows exponentially with `number`.

```
curl "http://127.0.0.1:3000/stress?number=40"
```

### `/delay`

Answers after `ms` milliseconds. The wait does not block the server, so other requests are served normally.

| Parameter | Default | Allowed range      |
|-----------|---------|--------------------|
| `ms`      | `1000`  | `0..MAX_DELAY_MS`  |

```
curl "http://127.0.0.1:3000/delay?ms=3000"
```

### `/status/:code`

Answers with the HTTP status you ask for. Codes must be integers from `200` to `599`, otherwise you get a `400`.

```
curl -i http://127.0.0.1:3000/status/503
```

### `/memory`

Allocates `mb` megabytes and holds them for `hold` seconds, then releases them. It answers as soon as the memory is allocated, so you can chain requests.

| Parameter | Default | Allowed range           |
|-----------|---------|-------------------------|
| `mb`      | `100`   | `1..MAX_MEMORY_MB`      |
| `hold`    | `10`    | `0..MAX_HOLD_SECONDS`   |

`MAX_MEMORY_MB` is also the total that can be held at the same time: a request that would exceed it together with the memory already held gets a `429`. `hold=0` releases the memory right away.

The memory is filled synchronously, so a very large allocation pauses the server for a moment. The server process itself uses some memory on top of what you allocate.

To test an OOM kill, raise `MAX_MEMORY_MB` above the memory limit of your container. With the default (`512`) you can't exhaust a larger one.

```
curl "http://127.0.0.1:3000/memory?mb=200&hold=30"
```

### `/disk`

Writes `mb` megabytes to a file in `DISK_DIR` and deletes it after `hold` seconds. It answers once the data is on disk.

| Parameter | Default | Allowed range           |
|-----------|---------|-------------------------|
| `mb`      | `100`   | `1..MAX_DISK_MB`        |
| `hold`    | `10`    | `0..MAX_HOLD_SECONDS`   |

`MAX_DISK_MB` is the total that can be held at the same time, with the same `400` and `429` rules as `/memory`. If the write fails (disk full, read-only filesystem, directory not writable) the answer is a `507`.

Files are removed on shutdown, and leftovers from a previous run are removed at startup.

```
curl "http://127.0.0.1:3000/disk?mb=200&hold=30"
```

## Configuration

Environment variables:

| Variable           | Default         | Description                                       |
|--------------------|-----------------|---------------------------------------------------|
| `PORT`             | `3000`          | Port the server listens on                        |
| `WORKERS`          | number of cores | Number of worker processes for `/stress`          |
| `MAX_NUMBER`       | `50`            | Highest `number` accepted by `/stress`            |
| `MAX_DELAY_MS`     | `60000`         | Highest `ms` accepted by `/delay`                 |
| `MAX_MEMORY_MB`    | `512`           | Total memory `/memory` can hold at once           |
| `MAX_DISK_MB`      | `1024`          | Total disk space `/disk` can hold at once         |
| `MAX_HOLD_SECONDS` | `300`           | Highest `hold` accepted by `/memory` and `/disk`  |
| `DISK_DIR`         | system temp dir | Directory where `/disk` writes its files          |

```
docker run -p 3000:3000 -e WORKERS=2 -e MAX_MEMORY_MB=2048 altrdev/stress-node:latest
```

## Docker

The image is built for `linux/amd64` and `linux/arm64`, runs as the non-root `node` user and has a `HEALTHCHECK` on `/`. Because of the non-root user, a directory you use as `DISK_DIR` (for example a mounted volume) must be writable by it.

## How it works

`/stress` requests are spread round-robin over a pool of worker processes (one per core by default), so concurrent requests can saturate multiple cores. A worker that crashes is restarted automatically, and the request it was serving gets a `500`. The other endpoints run in the main process.

## Load testing

To keep several cores busy you need concurrent requests, at least as many as `WORKERS`. With [hey](https://github.com/rakyll/hey):

```
hey -n 100 -c 8 "http://127.0.0.1:3000/stress?number=40"
```

With [k6](https://k6.io), save this as `stress.js`:

```js
import http from 'k6/http';

export const options = { vus: 8, duration: '60s' };

export default function () {
    http.get('http://127.0.0.1:3000/stress?number=40');
}
```

and run `k6 run stress.js`.

With plain `curl` (8 parallel requests):

```
for i in $(seq 8); do curl -s "http://127.0.0.1:3000/stress?number=40" & done; wait
```

Lower or raise `number` to tune how long each request keeps a core busy.

## Changelog

See [CHANGELOG.md](https://github.com/altrdev/stress-node/blob/main/CHANGELOG.md) and the [GitHub releases](https://github.com/altrdev/stress-node/releases).

## License

MIT
