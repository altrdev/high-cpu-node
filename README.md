# stress-node

![stress-node logo](https://raw.githubusercontent.com/altrdev/stress-node/main/docs/logo.png)

[![CI](https://github.com/altrdev/stress-node/actions/workflows/ci.yml/badge.svg)](https://github.com/altrdev/stress-node/actions/workflows/ci.yml)
[![Docker image](https://img.shields.io/docker/v/altrdev/stress-node?sort=semver&label=docker%20image)](https://hub.docker.com/r/altrdev/stress-node)
[![Docker pulls](https://img.shields.io/docker/pulls/altrdev/stress-node)](https://hub.docker.com/r/altrdev/stress-node)
[![License](https://img.shields.io/github/license/altrdev/stress-node)](https://github.com/altrdev/stress-node/blob/main/LICENSE)

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
| `/info`         |                    | JSON with what the server is doing and the remaining budgets |

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

### `/info`

Returns a JSON document with what the server is doing right now. It takes no parameters and doesn't use up any budget, so you can poll it while you test, for example with `watch -n1 curl -s http://127.0.0.1:3000/info`.

```json
{
  "name": "stress-node",
  "version": "2.4.0",
  "uptimeSeconds": 321,
  "stress": { "workers": 4, "inFlight": 2, "perWorker": [1, 1, 0, 0] },
  "delay": { "inFlight": 3 },
  "memory": { "heldMb": 200, "limitMb": 512, "processRssMb": 310, "containerLimitMb": 256 },
  "disk": { "heldMb": 100, "limitMb": 1024, "dir": "/tmp", "freeMb": 8120 },
  "limits": { "maxNumber": 50, "maxDelayMs": 60000, "maxHoldSeconds": 300 }
}
```

| Field                         | Meaning                                                                 |
|-------------------------------|-------------------------------------------------------------------------|
| `stress.inFlight`, `perWorker`| `/stress` requests being calculated, in total and for each worker       |
| `delay.inFlight`              | `/delay` requests still waiting                                         |
| `memory.heldMb`, `limitMb`    | Memory held by `/memory` and the total it may hold (`MAX_MEMORY_MB`); the difference is what a new request can still allocate |
| `memory.processRssMb`         | Memory the whole process uses, the figure that counts for an OOM kill   |
| `memory.containerLimitMb`     | Memory limit of the container, `null` when there is none                |
| `disk.heldMb`, `limitMb`      | Same as for memory, for `/disk` (`MAX_DISK_MB`)                         |
| `disk.freeMb`                 | Free space in `DISK_DIR`, `null` if the directory is not usable         |

## Watching a server

`cli.js` shows what a server is doing in the terminal, refreshing every second with bars and a short history of the last samples. It reads [`/info`](#info), needs no extra dependency and works against any running instance:

```
npm run watch
npm run watch -- --url http://my-host:3000 --interval 2
```

![stress-node watch: bars and history of busy workers, held memory, process RSS and held disk](https://raw.githubusercontent.com/altrdev/stress-node/main/docs/watch.svg)

Inside a container, where the server is on `localhost`:

```
docker exec -it <container> node cli.js watch
```

| Option       | Default                    | Description                                              |
|--------------|----------------------------|----------------------------------------------------------|
| `--url`      | `http://127.0.0.1:$PORT` or port `3000` | Server to watch                              |
| `--interval` | `1`                        | Seconds between samples                                  |
| `--once`     |                            | Print one sample and exit; the status is `1` if the server can't be reached |

The history only covers the time the command has been running. The colour of a bar goes from green to red as it gets close to its limit; set `NO_COLOR=1` to turn colours off.

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

## Examples

### Docker Compose

```yaml
services:
  stress:
    image: altrdev/stress-node:latest
    ports:
      - "3000:3000"
    environment:
      WORKERS: 2
      MAX_MEMORY_MB: 1024
    deploy:
      resources:
        limits:
          cpus: "1"
          memory: 512M
```

### Kubernetes

A pod with a memory limit and probes on `/`:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: stress-node
spec:
  replicas: 1
  selector:
    matchLabels:
      app: stress-node
  template:
    metadata:
      labels:
        app: stress-node
    spec:
      containers:
        - name: stress-node
          image: altrdev/stress-node:latest
          ports:
            - containerPort: 3000
          env:
            - name: WORKERS
              value: "1"
            - name: MAX_MEMORY_MB
              value: "1024"   # above the limit below, so /memory can trigger an OOM kill
          resources:
            requests:
              cpu: 250m
              memory: 128Mi
            limits:
              cpu: 500m
              memory: 256Mi
          readinessProbe:
            httpGet:
              path: /
              port: 3000
          livenessProbe:
            httpGet:
              path: /
              port: 3000
```

Some things to try, after `kubectl port-forward deploy/stress-node 3000:3000`:

- `/stress?number=45` on several terminals keeps the container at its CPU limit, which you can watch with `kubectl top pod` or use to trigger a CPU-based autoscaler.
- `/memory?mb=400` goes above the `256Mi` limit, so the container should be OOM killed and restarted.
- `/status/503` is a quick way to check how a probe or a retry policy reacts to errors.

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

## Security

This tool is meant to consume resources, so run it only in environments you control and don't expose it to the internet. If you find a security problem, please open an issue at https://github.com/altrdev/stress-node/issues.

## License

MIT
