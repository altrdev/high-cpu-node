# torchio

[![CI](https://github.com/altrdev/torchio/actions/workflows/ci.yml/badge.svg)](https://github.com/altrdev/torchio/actions/workflows/ci.yml) [![Docker image](https://img.shields.io/docker/v/altrdev/torchio.svg?sort=semver&label=docker%20image)](https://hub.docker.com/r/altrdev/torchio) [![Docker pulls](https://img.shields.io/docker/pulls/altrdev/torchio.svg)](https://hub.docker.com/r/altrdev/torchio) [![License](https://img.shields.io/github/license/altrdev/torchio)](https://github.com/altrdev/torchio/blob/main/LICENSE)

**Break your containers on purpose.** torchio is a small Node.js server that burns CPU, holds memory, fills disk, answers slowly or returns any HTTP status, all on demand over HTTP. Use it to see how your setup behaves under pressure: autoscaling, resource limits, OOM kills, probes, timeouts and retries.

*Torchio* is the Italian word for the press that squeezes wine and oil, as in *mettere sotto torchio*: to put under pressure. Put your containers under the press. It began in 2019 as `high-cpu-node`, when it could only burn CPU.

![torchio watch: bars and history of busy workers, held memory, process RSS and held disk, while a load runs](https://raw.githubusercontent.com/altrdev/torchio/refs/heads/main/docs/watch.svg)

Source code and issues: https://github.com/altrdev/torchio

> **Warning:** this server is meant to consume resources on purpose. Don't expose it to the public internet.

## Try it in 30 seconds

Start the server with Docker:

```
docker run --name torchio -p 3000:3000 altrdev/torchio
```

Or on your local machine (Node.js 22 or later):

```
git clone https://github.com/altrdev/torchio.git
cd torchio
npm install
npm start
```

In a second terminal, watch what it does:

```
docker exec -it torchio node cli.js watch    # with Docker
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
docker run -d --name oom -m 256m -e MAX_MEMORY_MB=1024 -p 3000:3000 altrdev/torchio
curl "http://127.0.0.1:3000/memory?mb=400"
docker inspect -f '{{.State.OOMKilled}}' oom
```

The container should be killed, and the last command prints `true`. `MAX_MEMORY_MB` is raised above the limit on purpose: its default (`512`) is a safety cap, so with it you can't exhaust a larger container.

### Autoscaling

A core stays busy only while a request is running, so send at least as many concurrent requests as workers (one per core by default):

```
hey -z 60s -c 8 "http://127.0.0.1:3000/stress?number=40"
```

To hold the CPU at a level for a while, without sending requests, use `/cpu`. A CPU based autoscaler reacts to a percentage, which this gives you directly:

```
curl "http://127.0.0.1:3000/cpu?percent=80&hold=300"
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

### Leaks and slow starts

```
curl "http://127.0.0.1:3000/leak?rate=10&mb=400&hold=60"
START_DELAY_SECONDS=30 npm start
```

`/leak` grows the memory by itself, a few MB each second, so you can watch a memory alert or an autoscaler react before the OOM kill. `START_DELAY_SECONDS` keeps the port closed for a while after the start, to test startup probes, rollouts and `depends_on`.

### Readiness, restarts and descriptors

```
curl "http://127.0.0.1:3000/ready/off?seconds=60"
curl -i http://127.0.0.1:3000/ready
curl "http://127.0.0.1:3000/crash?code=1"
curl "http://127.0.0.1:3000/fd?count=500&hold=30"
```

`/ready/off` makes `/ready` answer `503` for a while while `/` keeps answering `200`, so you can see an instance leave a Service or a load balancer without being restarted. `/crash` ends the process with the exit code you choose, to watch restarts and backoff. `/fd` holds file descriptors open, to see what a process does when it runs out of them.

### Metrics and dashboards

[`/info`](#info) is a quick look at what the server holds and what is left of each budget, and [`/metrics`](#metrics) has the same figures for Prometheus.

## Endpoints

The server listens on port 3000.

| Endpoint        | Parameters         | Description                                                 |
|-----------------|--------------------|-------------------------------------------------------------|
| `/`             |                    | Health check, returns `Status: OK`                          |
| `/stress`       | `number`           | Runs a Fibonacci calculation to load a CPU                  |
| `/cpu`          | `percent`, `hold`  | Keeps the workers busy for a share of the time, to test CPU based autoscaling |
| `/delay`        | `ms`               | Answers after a delay, to test timeouts and slow backends   |
| `/status/:code` | `code` (in the path) | Answers with the given HTTP status, to test probes and retries |
| `/memory`       | `mb`, `hold`       | Allocates memory and holds it, to test memory limits and OOM kills |
| `/leak`         | `rate`, `mb`, `hold` | Memory that grows by itself up to a ceiling, to test alerts and leaks |
| `/disk`         | `mb`, `hold`       | Writes a file and keeps it, to test disk limits and volumes |
| `/ready`        |                    | Readiness check: `200`, or `503` after `/ready/off`         |
| `/ready/off`    | `seconds`          | Makes `/ready` answer `503`, to test readiness probes       |
| `/ready/on`     |                    | Makes `/ready` answer `200` again                           |
| `/crash`        | `code`, `after`    | Ends the process, to test restarts and backoff              |
| `/fd`           | `count`, `hold`    | Opens file descriptors and holds them, to test `EMFILE`     |
| `/ui`           |                    | Control panel in the browser, only with `UI=1`              |
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

### `/cpu`

Use [`/stress`](#stress) when the load must come from requests, so that response times show the pressure and concurrent requests queue up; use `/cpu` when you only need the CPU held at a level, as a percentage, for a while.

Keeps every worker busy for `percent` of the time during `hold` seconds, instead of running one calculation. With one worker for each core (the default), `percent` is the load of the machine, which is what a CPU based autoscaler looks at. A new request replaces the previous one, and `percent=0` stops it.

| Parameter | Default | Allowed range         |
|-----------|---------|-----------------------|
| `percent` | `50`    | `0..100`              |
| `hold`    | `10`    | `0..MAX_HOLD_SECONDS` |

The answer comes right away. `/stress` keeps working while `/cpu` runs, and what is left is in `/info` (`stress.burn`) and in `/metrics` (`torchio_cpu_burn_percent`). In a container with a CPU limit the workers compete for it, so set `WORKERS` in line with the limit.

```
curl "http://127.0.0.1:3000/cpu?percent=70&hold=120"
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

### `/leak`

Allocates memory in steps of `rate` MB each second up to `mb` MB, keeps it for `hold` seconds after the last step and then releases it. It answers right away. It is a leak you can follow from the outside: `/info`, `/metrics` and `watch` show the memory growing.

| Parameter | Default | Allowed range         |
|-----------|---------|-----------------------|
| `rate`    | `10`    | `1..MAX_MEMORY_MB`    |
| `mb`      | `100`   | `1..MAX_MEMORY_MB`    |
| `hold`    | `10`    | `0..MAX_HOLD_SECONDS` |

It takes its memory from the same budget as [`/memory`](#memory) (`MAX_MEMORY_MB`) a step at a time. If the budget is already full the first request gets a `429`, and a leak that fills it while growing stops there and is held until `hold` ends.

```
curl "http://127.0.0.1:3000/leak?rate=20&mb=500&hold=60"
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

### `/ready`

A readiness check that you can switch. It answers `200` (`Ready`) until you call `/ready/off`, then `503` (`Not ready`). The liveness check `/` is not affected, so a container that is not ready stays alive.

| Endpoint     | Parameter | Default | Allowed range         | Effect                                                    |
|--------------|-----------|---------|-----------------------|-----------------------------------------------------------|
| `/ready/off` | `seconds` | `30`    | `0..MAX_HOLD_SECONDS` | `/ready` answers `503` for that long, `0` until `/ready/on` |
| `/ready/on`  |           |         |                       | `/ready` answers `200` again                              |

The state is in `/info` (`ready`) and in `/metrics` (`torchio_ready`). Calling `/ready/off` again restarts the countdown.

```
curl "http://127.0.0.1:3000/ready/off?seconds=60"
curl -i http://127.0.0.1:3000/ready
```

### `/crash`

Answers, then ends the whole process with the exit code you choose. Use it to test restart policies, backoff and `CrashLoopBackOff`. Nothing is cleaned up on the way out, like in a real crash; the files of `/disk` left behind are removed at the next start.

| Parameter | Default | Allowed range         |
|-----------|---------|-----------------------|
| `code`    | `1`     | `0..255`              |
| `after`   | `0`     | `0..MAX_HOLD_SECONDS` seconds |

```
curl "http://127.0.0.1:3000/crash?code=137&after=5"
```

Remember that anyone who can reach the server can stop it. As with the rest, don't expose it to the internet.

### `/fd`

Opens `count` file descriptors and closes them after `hold` seconds.

| Parameter | Default | Allowed range         |
|-----------|---------|-----------------------|
| `count`   | `100`   | `1..MAX_FD`           |
| `hold`    | `10`    | `0..MAX_HOLD_SECONDS` |

`MAX_FD` is the total that can be held at the same time, with the same `400` and `429` rules as `/memory`. The limit of the process (`ulimit -n`) is usually lower than `MAX_FD` in a container that sets one: when it is reached the answer is a `503` that says how many could be opened, and those stay open until `hold` ends. While they are open the server may fail to accept new connections or to open files, which is the situation being tested.

```
curl "http://127.0.0.1:3000/fd?count=2000&hold=30"
```

### `/info`

Returns a JSON document with what the server is doing right now. It takes no parameters and doesn't use up any budget, so you can poll it while you test, for example with `watch -n1 curl -s http://127.0.0.1:3000/info`.

```json
{
  "name": "torchio",
  "version": "3.3.1",
  "uptimeSeconds": 321,
  "stress": { "workers": 4, "inFlight": 2, "perWorker": [1, 1, 0, 0], "burn": null },
  "delay": { "inFlight": 3 },
  "memory": { "heldMb": 200, "limitMb": 512, "processRssMb": 310, "containerLimitMb": 256 },
  "disk": { "heldMb": 100, "limitMb": 1024, "dir": "/tmp", "freeMb": 8120 },
  "fd": { "held": 0, "limit": 10000 },
  "ready": { "ready": true, "secondsLeft": null },
  "limits": { "maxNumber": 50, "maxDelayMs": 60000, "maxHoldSeconds": 300 }
}
```

| Field                         | Meaning                                                                 |
|-------------------------------|-------------------------------------------------------------------------|
| `stress.inFlight`, `perWorker`| `/stress` requests being calculated, in total and for each worker       |
| `stress.burn`                 | What `/cpu` is doing, `{ "percent", "secondsLeft" }`, or `null` when it is not running |
| `delay.inFlight`              | `/delay` requests still waiting                                         |
| `memory.heldMb`, `limitMb`    | Memory held by `/memory` and the total it may hold (`MAX_MEMORY_MB`); the difference is what a new request can still allocate |
| `memory.processRssMb`         | Memory the whole process uses, the figure that counts for an OOM kill   |
| `memory.containerLimitMb`     | Memory limit of the container, `null` when there is none                |
| `disk.heldMb`, `limitMb`      | Same as for memory, for `/disk` (`MAX_DISK_MB`)                         |
| `disk.freeMb`                 | Free space in `DISK_DIR`, `null` if the directory is not usable         |
| `fd.held`, `limit`            | File descriptors held by `/fd` and the total it may hold (`MAX_FD`)     |
| `ready.ready`, `secondsLeft`  | What `/ready` answers, and the seconds left before it is ready again (`null` if nothing is counting down) |

### `/metrics`

The figures of [`/info`](#info) in the Prometheus text format, plus a counter of the requests answered. It takes no parameters and doesn't use up any budget.

| Metric                                      | Type    | Meaning                                                     |
|---------------------------------------------|---------|-------------------------------------------------------------|
| `torchio_info{version}`                 | gauge   | Always `1`, the version is in the label                     |
| `torchio_start_time_seconds`            | gauge   | Start time, in Unix time                                    |
| `torchio_workers`                       | gauge   | Worker processes for `/stress`                              |
| `torchio_stress_requests_in_flight`     | gauge   | `/stress` requests being calculated                         |
| `torchio_worker_requests_in_flight{worker}` | gauge | The same, for each worker                                 |
| `torchio_cpu_burn_percent`              | gauge   | Share of the time each worker is kept busy by `/cpu`, `0` when it is not running |
| `torchio_delay_requests_in_flight`      | gauge   | `/delay` requests still waiting                             |
| `torchio_memory_held_bytes`             | gauge   | Memory held by `/memory`                                    |
| `torchio_memory_limit_bytes`            | gauge   | Total memory `/memory` may hold (`MAX_MEMORY_MB`)           |
| `process_resident_memory_bytes`             | gauge   | Resident memory of the whole process                        |
| `torchio_container_memory_limit_bytes`  | gauge   | Memory limit of the container, only present when there is one |
| `torchio_disk_held_bytes`               | gauge   | Disk space held by `/disk`                                  |
| `torchio_disk_limit_bytes`              | gauge   | Total disk space `/disk` may hold (`MAX_DISK_MB`)           |
| `torchio_disk_free_bytes`               | gauge   | Free space in `DISK_DIR`, only present when it is usable    |
| `torchio_fd_held`                       | gauge   | File descriptors held by `/fd`                              |
| `torchio_fd_limit`                      | gauge   | Total descriptors `/fd` may hold (`MAX_FD`)                 |
| `torchio_ready`                         | gauge   | `1` while `/ready` answers `200`, `0` while it answers `503` |
| `torchio_http_requests_total{route,code}` | counter | Requests answered, by route pattern (such as `/status/:code`) and status code. A request the client abandons before the answer is not counted |

To collect them with Prometheus:

```yaml
scrape_configs:
  - job_name: torchio
    static_configs:
      - targets: ["torchio:3000"]
```

Some queries to start from:

- `torchio_memory_held_bytes / torchio_memory_limit_bytes` is how much of the `/memory` budget is used.
- `process_resident_memory_bytes / torchio_container_memory_limit_bytes` is how close the process is to an OOM kill.
- `rate(torchio_http_requests_total{code=~"4..|5.."}[1m])` is the rate of rejected and failed requests (`429`, `507`, `/status/...`).

## Watching a server

`cli.js` shows what a server is doing in the terminal, refreshing every second with bars and a short history of the last samples (workers, memory, process memory, disk and file descriptors), plus whether `/ready` is answering `200` or `503`. It reads [`/info`](#info), needs no extra dependency and works against any running instance:

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

A server older than 3.1 doesn't report descriptors and readiness, so those two lines are left out. The history only covers the time the command has been running. The colour of a bar goes from green to red as it gets close to its limit; set `NO_COLOR=1` to turn colours off.

## Control panel

`torchio` also has a page in the browser that does the job of `watch` and lets you start the tests with sliders and buttons instead of `curl`. It is off by default, turn it on with `UI=1` and open `/ui`:

```
docker run -p 3000:3000 -e UI=1 altrdev/torchio:latest
```

![The torchio control panel: gauges with history for workers, memory, process RSS, disk, descriptors and requests, and a card for each test](https://raw.githubusercontent.com/altrdev/torchio/refs/heads/main/docs/ui.png)

The gauges and their history (the last 60 seconds) come from [`/info`](#info) and [`/metrics`](#metrics), refreshed every second, and the page shows whether `/ready` is answering `200` or `503`. Each card makes the request you see written under its sliders, and the answer goes in the log at the bottom, so it is also a way to learn the API. The history lives in the page: reloading it starts again.

The page makes [`/crash`](#crash) a button, which is why it is off by default (and the button asks to be clicked twice). It needs no extra dependency and no build, it is one static file.

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
| `UI`               | off             | `1` turns on the control panel at `/ui`           |
| `START_DELAY_SECONDS` | `0`         | Seconds the port stays closed after the start     |
| `MAX_FD`           | `10000`         | Total descriptors `/fd` can hold at once          |
| `MAX_HOLD_SECONDS` | `300`           | Highest `hold`, `seconds` and `after` accepted by the endpoints that take them |
| `DISK_DIR`         | system temp dir | Directory where `/disk` writes its files          |

A number must be a positive integer. A value that is not one is reported at startup and replaced by the default.

```
docker run -p 3000:3000 -e WORKERS=2 -e MAX_MEMORY_MB=2048 altrdev/torchio:latest
```

## Docker

The image is built for `linux/amd64` and `linux/arm64`, runs as the non-root `node` user and has a `HEALTHCHECK` on `/`. With `START_DELAY_SECONDS` the container is unhealthy until the port opens, so give the healthcheck a longer `--start-period`. Because of the non-root user, a directory you use as `DISK_DIR` (for example a mounted volume) must be writable by it.

## Examples

### Docker Compose

```yaml
services:
  stress:
    image: altrdev/torchio:latest
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
  name: torchio
spec:
  replicas: 1
  selector:
    matchLabels:
      app: torchio
  template:
    metadata:
      labels:
        app: torchio
    spec:
      containers:
        - name: torchio
          image: altrdev/torchio:latest
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
              path: /ready
              port: 3000
          livenessProbe:
            httpGet:
              path: /
              port: 3000
```

Some things to try, after `kubectl port-forward deploy/torchio 3000:3000`:

- `/stress?number=45` on several terminals keeps the container at its CPU limit, which you can watch with `kubectl top pod` or use to trigger a CPU-based autoscaler.
- `/memory?mb=400` goes above the `256Mi` limit, so the container should be OOM killed and restarted.
- `/ready/off?seconds=60` takes the pod out of the Service endpoints for a minute without restarting it (the readiness probe now checks `/ready`; the liveness probe still checks `/`).
- `/crash?code=1` restarts the container, and calling it again each time shows the backoff growing.
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

The tests use `node:test`, which comes with Node.js, so there is nothing to install. They cover every endpoint and its limits, the budgets of `/memory`, `/disk` and `/fd`, `/cpu`, `/leak`, `START_DELAY_SECONDS`, the control panel, `/ready`, `/crash`, a worker that crashes, an unusable `DISK_DIR`, the cleanup of the files on startup and shutdown, `/metrics` and the `watch` command. CI also runs them on Node 22, 24 and 26, checks the Docker image and validates `/metrics` with `promtool`.

## Changelog

See [CHANGELOG.md](https://github.com/altrdev/torchio/blob/main/CHANGELOG.md) and the [GitHub releases](https://github.com/altrdev/torchio/releases).

## Security

This tool is meant to consume resources, so run it only in environments you control and don't expose it to the internet. If you find a security problem, please open an issue at https://github.com/altrdev/torchio/issues.

## License

MIT
