# stress-node

A simple Node.js server for stressing the CPU with a recursive Fibonacci calculation. Useful for testing autoscaling, load balancers and container CPU limits.

> **Warning:** this server is meant to burn CPU on purpose. Don't expose it to the public internet.

## Install

```
npm install
```

## Usage

On your local machine:

```
npm start
```

Or with Docker:

```
docker pull altrdev/stress-node:latest
docker run -p 3000:3000 altrdev/stress-node:latest
```

## Endpoints

The server listens on port 3000.

| Endpoint        | Description                                                  |
|-----------------|--------------------------------------------------------------|
| `/`             | Health check, returns `Status: OK`                           |
| `/stress`       | Runs a Fibonacci calculation to load a CPU                   |
| `/delay`        | Answers after a delay, to test timeouts and slow backends    |
| `/status/:code` | Answers with the given HTTP status, to test probes and retries |
| `/memory`       | Allocates memory and holds it for a while, to test memory limits and OOM kills |
| `/disk`         | Writes a file and keeps it for a while, to test disk limits and volumes |

`/stress` accepts an optional query string parameter `number`, the sequence length to calculate:

- if omitted or not a number, it defaults to `45`
- if outside the range `0..MAX_NUMBER`, the server answers `400`

The calculation time grows exponentially with `number`.

`/delay` accepts an optional `ms` parameter (default `1000`). The wait does not block the server, so other requests are served normally. A value outside `0..MAX_DELAY_MS` gets a `400`.

`/memory` allocates `mb` megabytes (default `100`) and holds them for `hold` seconds (default `10`, `0` releases immediately). It answers as soon as the memory is allocated, so you can chain requests. `MAX_MEMORY_MB` is the total that can be held at the same time: a single request above it gets a `400`, one that would exceed it together with memory already held gets a `429`. `hold` must be between `0` and `MAX_HOLD_SECONDS`.

`/disk` writes `mb` megabytes (default `100`) to a file in `DISK_DIR` and deletes it after `hold` seconds (default `10`, `0` deletes it right away). It answers once the data is on disk. `MAX_DISK_MB` is the total that can be held at the same time, with the same `400`/`429` rules as `/memory`. If the write fails (disk full, read-only filesystem, directory not writable) the answer is a `507`. Files are removed on shutdown, and leftovers from a previous run are removed at startup. Point `DISK_DIR` to a mounted volume to test it.

`/status/:code` answers with the status you ask for, for example `/status/503`. Codes must be integers from `200` to `599`, otherwise you get a `400`.

```
http://127.0.0.1:3000/stress
http://127.0.0.1:3000/stress?number=40
```

## How it works

Requests are spread round-robin over a pool of worker processes (one per core by default), so concurrent requests can saturate multiple cores. A worker that crashes is restarted automatically, and the request it was serving gets a `500`.

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

## Configuration

Environment variables:

| Variable     | Default         | Description                            |
|--------------|-----------------|----------------------------------------|
| `PORT`       | `3000`          | Port the server listens on             |
| `WORKERS`    | number of cores | Number of worker processes             |
| `MAX_NUMBER` | `50`            | Highest `number` accepted by `/stress` |
| `MAX_DELAY_MS` | `60000`       | Highest `ms` accepted by `/delay`      |
| `MAX_MEMORY_MB` | `512`        | Total memory `/memory` can hold at once |
| `MAX_HOLD_SECONDS` | `300`     | Highest `hold` accepted by `/memory` and `/disk` |
| `MAX_DISK_MB`  | `1024`        | Total disk space `/disk` can hold at once |
| `DISK_DIR`     | system temp dir | Directory where `/disk` writes its files |

```
docker run -p 3000:3000 -e WORKERS=2 altrdev/stress-node:latest
```

## Changelog

- **2.2.1**: Project renamed to `stress-node`. The Docker image is published as `altrdev/stress-node`; `altrdev/high-cpu-node` is deprecated and 2.2.1 is its last release.
- **2.2.0**: Updated to Express 5, Node 26 base image and `cors` 2.8.6.
- **2.1.0**: Docker image has a `HEALTHCHECK`; the Docker Hub description is synced from this README on release.
- **2.0.0**: `/stress` rejects `number` outside `0..MAX_NUMBER` with `400` (previously unbounded). Added the worker pool and the `PORT`, `WORKERS`, `MAX_NUMBER` variables.

## License

MIT
