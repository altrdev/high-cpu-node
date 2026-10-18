# stress-node

[![CI](https://github.com/altrdev/stress-node/actions/workflows/ci.yml/badge.svg)](https://github.com/altrdev/stress-node/actions/workflows/ci.yml) [![Docker image](https://img.shields.io/docker/v/altrdev/stress-node?sort=semver&label=docker%20image)](https://hub.docker.com/r/altrdev/stress-node) [![Docker pulls](https://img.shields.io/docker/pulls/altrdev/stress-node)](https://hub.docker.com/r/altrdev/stress-node) [![License](https://img.shields.io/github/license/altrdev/stress-node)](https://github.com/altrdev/stress-node/blob/main/LICENSE)

**Break your containers on purpose.** stress-node is a small Node.js server that burns CPU, holds memory, fills disk, answers slowly or returns any HTTP status, all on demand over HTTP. Use it to see how your setup behaves under pressure: autoscaling, resource limits, OOM kills, probes, timeouts and retries.

![stress-node watch: bars and history of busy workers, held memory, process RSS and held disk, while a load runs](https://raw.githubusercontent.com/altrdev/stress-node/refs/heads/main/docs/watch.svg)

Source code and issues: https://github.com/altrdev/stress-node

> **Warning:** this server is meant to consume resources on purpose. Don't expose it to the public internet.

## Try it in 30 seconds

Start the server with Docker:

```
docker run --name stress -p 3000:3000 altrdev/stress-node
```

Or on your local machine (Node.js 22 or later):

```
git clone https://github.com/altrdev/stress-node.git
cd stress-node
npm install
npm start
```

In a second terminal, watch what it does:

```
docker exec -it stress node cli.js watch    # with Docker
npm run watch                               # on your local machine
```

In a third one, put it under load, and see the bars move:

```
curl "http://127.0.0.1:3000/memory?mb=200&hold=20"
curl "http://127.0.0.1:3000/disk?mb=100&hold=20"
for i in 1 2; do curl -s "http://127.0.0.1:3000/stress?number=44" & done; wait
```

## What you can test

### Memory limits and OOM kills

Give the container a memory limit and ask for more than that:

```
docker run -d --name oom -m 256m -e MAX_MEMORY_MB=1024 -p 3000:3000 altrdev/stress-node
curl "http://127.0.0.1:3000/memory?mb=400"
docker inspect -f '{{.State.OOMKilled}}' oom
```

The container should be killed, and the last command prints `true`. `MAX_MEMORY_MB` is raised above the limit on purpose: its default (`512`) is a safety cap, so with it you can't exhaust a larger container.

### Autoscaling

A core stays busy only while a request is running, so send at least as many concurrent requests as workers (one per core by default):

```
hey -z 60s -c 8 "http://127.0.0.1:3000/stress?number=40"
```

More about it in [Load testing](#load-testing).

### Probes, timeouts and retries

```
curl -i http://127.0.0.1:3000/status/503
curl --max-time 2 "http://127.0.0.1:3000/delay?ms=5000"
```

`/status/:code` answers with any status from `200` to `599`, and `/delay` waits without blocking the server, so the rest keeps being served while a slow request is pending.

### Disk and volumes

```
curl "http://127.0.0.1:3000/disk?mb=500&hold=60"
```

If the write fails (disk full, read-only filesystem, directory not writable) the answer is a `507`. Point `DISK_DIR` to a mounted volume to test it.

### Metrics and dashboards

[`/info`](#info) is a quick look at what the server holds and what is left of each budget, and [`/metrics`](#metrics) has the same figures for Prometheus.

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
| `/metrics`      |                    | The same figures in the Prometheus text format               |

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
  "version": "2.5.3",
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

### `/metrics`

The figures of [`/info`](#info) in the Prometheus text format, plus a counter of the requests answered. It takes no parameters and doesn't use up any budget.

| Metric                                      | Type    | Meaning                                                     |
|---------------------------------------------|---------|-------------------------------------------------------------|
| `stress_node_info{version}`                 | gauge   | Always `1`, the version is in the label                     |
| `stress_node_start_time_seconds`            | gauge   | Start time, in Unix time                                    |
| `stress_node_workers`                       | gauge   | Worker processes for `/stress`                              |
| `stress_node_stress_requests_in_flight`     | gauge   | `/stress` requests being calculated                         |
| `stress_node_worker_requests_in_flight{worker}` | gauge | The same, for each worker                                 |
| `stress_node_delay_requests_in_flight`      | gauge   | `/delay` requests still waiting                             |
| `stress_node_memory_held_bytes`             | gauge   | Memory held by `/memory`                                    |
| `stress_node_memory_limit_bytes`            | gauge   | Total memory `/memory` may hold (`MAX_MEMORY_MB`)           |
| `process_resident_memory_bytes`             | gauge   | Resident memory of the whole process                        |
| `stress_node_container_memory_limit_bytes`  | gauge   | Memory limit of the container, only present when there is one |
| `stress_node_disk_held_bytes`               | gauge   | Disk space held by `/disk`                                  |
| `stress_node_disk_limit_bytes`              | gauge   | Total disk space `/disk` may hold (`MAX_DISK_MB`)           |
| `stress_node_disk_free_bytes`               | gauge   | Free space in `DISK_DIR`, only present when it is usable    |
| `stress_node_http_requests_total{route,code}` | counter | Requests answered, by route pattern (such as `/status/:code`) and status code. A request the client abandons before the answer is not counted |

To collect them with Prometheus:

```yaml
scrape_configs:
  - job_name: stress-node
    static_configs:
      - targets: ["stress-node:3000"]
```

Some queries to start from:

- `stress_node_memory_held_bytes / stress_node_memory_limit_bytes` is how much of the `/memory` budget is used.
- `process_resident_memory_bytes / stress_node_container_memory_limit_bytes` is how close the process is to an OOM kill.
- `rate(stress_node_http_requests_total{code=~"4..|5.."}[1m])` is the rate of rejected and failed requests (`429`, `507`, `/status/...`).

## Watching a server

`cli.js` shows what a server is doing in the terminal, refreshing every second with bars and a short history of the last samples. It reads [`/info`](#info), needs no extra dependency and works against any running instance:

```
npm run watch
npm run watch -- --url http://my-host:3000 --interval 2
```

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

A number must be a positive integer. A value that is not one is reported at startup and replaced by the default.

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

## Development

```
npm run dev     # restarts the server when a file changes
npm run watch   # terminal view of a running server
npm test        # starts real servers on free ports and checks them
```

```
server.js          starts the server (what npm start and the Docker image run)
cli.js             the watch command
src/
  app.js           builds the Express app and wires the routes
  config.js        environment variables, validated
  params.js        reads and validates the query parameters
  budget.js        the total that /memory and /disk share between requests
  workers.js       the pool of worker processes for /stress
  fibonacci.js     what a worker runs
  snapshot.js      the figures behind /info and /metrics
  counters.js      requests answered, for /metrics
  routes/          one file for each endpoint
test/              the tests
```

The tests use `node:test`, which comes with Node.js, so there is nothing to install. They cover every endpoint and its limits, the budgets of `/memory` and `/disk`, a worker that crashes, an unusable `DISK_DIR`, the cleanup of the files on startup and shutdown, `/metrics` and the `watch` command. CI also runs them on Node 22, 24 and 26, checks the Docker image and validates `/metrics` with `promtool`.

## Changelog

See [CHANGELOG.md](https://github.com/altrdev/stress-node/blob/main/CHANGELOG.md) and the [GitHub releases](https://github.com/altrdev/stress-node/releases).

## Security

This tool is meant to consume resources, so run it only in environments you control and don't expose it to the internet. If you find a security problem, please open an issue at https://github.com/altrdev/stress-node/issues.

## License

MIT
