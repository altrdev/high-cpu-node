# Endpoints

The server listens on port 3000.

| Endpoint        | Parameters         | Description                                                 |
|-----------------|--------------------|-------------------------------------------------------------|
| `/`             |                    | Health check, returns `Status: OK`                          |
| `/stress`       | `number`           | Runs a Fibonacci calculation to load a CPU                  |
| `/cpu`          | `percent`, `hold`, `workers` | Keeps the workers busy for a share of the time, to test CPU based autoscaling |
| `/delay`        | `ms`               | Answers after a delay, to test timeouts and slow backends   |
| `/status/:code` | `code` (in the path) | Answers with the given HTTP status, to test probes and retries |
| `/flaky`        | `rate`, `code`, `ms` | Fails a share of the requests, to test retries and circuit breakers |
| `/memory`       | `mb`, `hold`       | Allocates memory and holds it, to test memory limits and OOM kills |
| `/leak`         | `rate`, `mb`, `hold` | Memory that grows by itself up to a ceiling, to test alerts and leaks |
| `/disk`         | `mb`, `hold`       | Writes a file and keeps it, to test disk limits and volumes |
| `/ready`        |                    | Readiness check: `200`, or `503` after `/ready/off`         |
| `/ready/off`    | `seconds`          | Makes `/ready` answer `503`, to test readiness probes       |
| `/ready/on`     |                    | Makes `/ready` answer `200` again                           |
| `/crash`        | `code`, `after`    | Ends the process, to test restarts and backoff              |
| `/block`        | `ms`               | Freezes every thread of the server, to test liveness probes and timeouts |
| `/oom`          |                    | Allocates until the kernel kills the process, only with `ALLOW_OOM=1` |
| `/fd`           | `count`, `hold`    | Opens file descriptors and holds them, to test `EMFILE`     |
| `/connections`  | `count`, `hold`    | Opens TCP connections to the server and holds them, to test connection limits |
| `/log`          | `lines`, `rate`    | Writes lines to stdout at a rate, to test log collection and rotation |
| `/echo`         |                    | The request as the server sees it, to see what proxies add or remove |
| `/ui`           |                    | Control panel in the browser, only with `UI=1`              |
| `/info`         |                    | JSON with what the server is doing and the remaining budgets |
| `/metrics`      |                    | The same figures in the Prometheus text format               |
| `/help`         |                    | Every endpoint with its parameters, defaults and limits      |
| `/config`       |                    | The effective configuration, without the token               |

Every query parameter is optional. A value that is not a number falls back to its default; a number outside the allowed range gets a `400`. All the limits are configurable, see [Configuration](configuration.md#configuration).

## `/stress`

Runs a recursive Fibonacci calculation in a worker thread, so it keeps one core busy for the whole request.

| Parameter | Default | Allowed range      |
|-----------|---------|--------------------|
| `number`  | `45`    | `0..MAX_NUMBER`    |

The calculation time grows exponentially with `number`.

```
curl "http://127.0.0.1:3000/stress?number=40"
```

## `/cpu`

Use [`/stress`](#stress) when the load must come from requests, so that response times show the pressure and concurrent requests queue up; use `/cpu` when you only need the CPU held at a level, as a percentage, for a while.

Keeps the workers busy for `percent` of the time during `hold` seconds, instead of running one calculation. With one worker for each CPU (the default), `percent` is the load of the machine, which is what a CPU based autoscaler looks at. `workers` loads only the first ones and leaves the others idle, for an uneven load across the cores. A new request replaces the previous one, and `percent=0` stops it.

| Parameter | Default | Allowed range         |
|-----------|---------|-----------------------|
| `percent` | `50`    | `0..100`              |
| `hold`    | `10`    | `0..MAX_HOLD_SECONDS` |
| `workers` | all of them | `1..WORKERS`      |

The answer comes right away. `/stress` keeps working while `/cpu` runs, and what is left is in `/info` (`stress.burn`, with how many workers are loaded) and in `/metrics` (`torchio_cpu_burn_percent` and `torchio_cpu_burn_workers`). The default number of workers is the number of CPUs the process can use, so a CPU limit of the container counts (a limit of 2 CPUs gives 2 workers); set `WORKERS` to choose another.

```
curl "http://127.0.0.1:3000/cpu?percent=70&hold=120"
curl "http://127.0.0.1:3000/cpu?percent=90&workers=1"   # one core at 90%, the others idle
```

## `/delay`

Answers after `ms` milliseconds. The wait does not block the server, so other requests are served normally.

| Parameter | Default | Allowed range      |
|-----------|---------|--------------------|
| `ms`      | `1000`  | `0..MAX_DELAY_MS`  |

```
curl "http://127.0.0.1:3000/delay?ms=3000"
```

## `/status/:code`

Answers with the HTTP status you ask for. Codes must be integers from `200` to `599`, otherwise you get a `400`.

```
curl -i http://127.0.0.1:3000/status/503
```

## `/flaky`

Answers with `code` for `rate` percent of the requests and with `200` for the rest, to test retries, circuit breakers and outlier detection. Each request is decided on its own, so the share is only exact over many requests; `rate=0` never fails and `rate=100` always does. A failing answer waits `ms` milliseconds first, like [`/delay`](#delay), and a request that succeeds never waits.

| Parameter | Default | Allowed range     |
|-----------|---------|-------------------|
| `rate`    | `50`    | `0..100`          |
| `code`    | `503`   | `400..599`        |
| `ms`      | `0`     | `0..MAX_DELAY_MS` |

```
curl -i "http://127.0.0.1:3000/flaky?rate=30"
curl -i "http://127.0.0.1:3000/flaky?rate=100&code=429&ms=500"
```

The answers are counted in [`/metrics`](#metrics) by status as usual, so `torchio_http_requests_total{route="/flaky",code="503"}` is the number of failures. Like `/status/:code` and `/delay`, it changes nothing in the server, so [`TOKEN`](token.md#token) does not protect it.

## `/memory`

Allocates `mb` megabytes and holds them for `hold` seconds, then releases them. It answers as soon as the memory is allocated, so you can chain requests.

| Parameter | Default | Allowed range           |
|-----------|---------|-------------------------|
| `mb`      | `100`   | `1..MAX_MEMORY_MB`      |
| `hold`    | `10`    | `0..MAX_HOLD_SECONDS`   |

`MAX_MEMORY_MB` is also the total that can be held at the same time: a request that would exceed it together with the memory already held gets a `429`. `hold=0` releases the memory right away.

Every page is written, so the memory counts as used (RSS) as soon as the request is answered, and it goes back to the OS when it is released. The server itself needs only a few MB on top of what you allocate.

To test an OOM kill, raise `MAX_MEMORY_MB` above the memory limit of your container. With the default (`512`) you can't exhaust a larger one.

```
curl "http://127.0.0.1:3000/memory?mb=200&hold=30"
```

## `/leak`

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

## `/disk`

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

## `/ready`

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

## `/crash`

Answers, then ends the whole process with the exit code you choose. Use it to test restart policies, backoff and `CrashLoopBackOff`. Nothing is cleaned up on the way out, like in a real crash; the files of `/disk` left behind are removed at the next start.

| Parameter | Default | Allowed range         |
|-----------|---------|-----------------------|
| `code`    | `1`     | `0..255`              |
| `after`   | `0`     | `0..MAX_HOLD_SECONDS` seconds |

```
curl "http://127.0.0.1:3000/crash?code=137&after=5"
```

Remember that anyone who can reach the server can stop it. As with the rest, don't expose it to the internet.

## `/block`

Freezes the server: after answering, every thread of the runtime goes to sleep for `ms`, so the process is alive but accepts nothing and answers nothing until the time is over. Use it to test liveness probes, client timeouts and what a load balancer does with an instance that hangs. `/stress` and `/cpu` run in threads of their own and don't count.

| Parameter | Default | Allowed range         |
|-----------|---------|-----------------------|
| `ms`      | `5000`  | `1..MAX_BLOCK_MS`     |

The answer is sent first, so the request that starts the block returns at once; the next ones wait. A new `/block` can't start while one is going on, it is answered with a `409`. `/info` doesn't answer during the block either, but it counts them afterwards (`block`), and the log says when it starts and when the server answers again.

```
curl "http://127.0.0.1:3000/block?ms=10000"
curl -m 2 http://127.0.0.1:3000/    # times out for the next 10 seconds
```

## `/oom`

Allocates memory without any budget, a chunk of 64 MB at a time, until the kernel kills the process, to test what a container does when it is killed for memory: the `OOMKilled` status, the exit code `137` and the restart policy. `/memory` and `/leak` stop at `MAX_MEMORY_MB` and never get there.

It needs `ALLOW_OOM=1` (otherwise the answer is a `403` and nothing is allocated) and, when `TOKEN` is set, the token. The answer is sent first, and a warning in the log says what is about to happen and which limit the container has.

```
docker run -p 3000:3000 --memory 256m -e ALLOW_OOM=1 altrdev/torchio:latest
curl http://127.0.0.1:3000/oom
docker inspect --format '{{.State.OOMKilled}} {{.State.ExitCode}}' <container>
```

Use it only in a container with a memory limit. Without one it fills the memory of the machine, and the kernel may kill another process before this one.

## `/fd`

Opens `count` file descriptors and closes them after `hold` seconds.

| Parameter | Default | Allowed range         |
|-----------|---------|-----------------------|
| `count`   | `100`   | `1..MAX_FD`           |
| `hold`    | `10`    | `0..MAX_HOLD_SECONDS` |

`MAX_FD` is the total that can be held at the same time, with the same `400` and `429` rules as `/memory`. The limit of the process (`ulimit -n`) is usually lower than `MAX_FD` in a container that sets one: when it is reached the answer is a `503` that says how many could be opened, and those stay open until `hold` ends. While they are open the server may fail to accept new connections or to open files, which is the situation being tested.

```
curl "http://127.0.0.1:3000/fd?count=2000&hold=30"
```

## `/connections`

Opens `count` TCP connections to the server itself and closes them after `hold` seconds. It is the same shape as [`/fd`](#fd), for what a load balancer, a proxy or a connection limit sees: a server with many open connections, which stay idle. The connections are also closed when the process ends, so a shutdown with open sockets is easy to try.

| Parameter | Default | Allowed range         |
|-----------|---------|-----------------------|
| `count`   | `10`    | `1..MAX_CONNECTIONS`  |
| `hold`    | `10`    | `0..MAX_HOLD_SECONDS` |

`MAX_CONNECTIONS` is the total that can be held at the same time, with the same `400` and `429` rules as `/fd`. Each connection takes a descriptor on the side that opens it and another on the side that accepts it, both in this process, so the limit of the process (`ulimit -n`) is reached at half of it: the answer is then a `503` that says how many could be opened, and those stay open until `hold` ends. The answer comes when the connections are open.

```
curl "http://127.0.0.1:3000/connections?count=200&hold=60"
```

## `/log`

Writes `lines` lines (default 1000) to stdout at `rate` lines each second (default 100), then stops, to test log collection pipelines, rotation and how fast the logs of a container fill a disk. Each line has a timestamp and its number: `2026-10-18T14:00:00.000Z torchio-log line 7 of 1000`. A new request replaces the one still running; the lines already written stay. `MAX_LOG_LINES` and `MAX_LOG_RATE` set the limits. While it writes, [`/info`](#info) has `log` (`{ "sent", "lines", "rate" }`, `null` otherwise), and `watch` and the control panel show it.

## `/echo`

Returns the request as this server sees it: method, URL, protocol, `hostname`, client address (`ip`, the `ips` chain and `remoteAddress`, the peer of the connection) and all the headers. Call it through an ingress controller, a proxy or a load balancer to see what they add, remove or rewrite (`Host`, `X-Forwarded-For`, `X-Forwarded-Proto`). `Authorization`, `Proxy-Authorization` and `Cookie` are shown as `[masked]`.

By default the `X-Forwarded-*` headers are not trusted: `ip` is the peer and `protocol` is `http`. Set `TRUST_PROXY` to let them decide: `true` trusts every proxy, a number trusts that many hops from the server, and a list of addresses, subnets (`10.0.0.0/8`) and the names `loopback`, `linklocal` and `uniquelocal` trusts those. The client is then the first address, walking back from the connection through `X-Forwarded-For`, that is not a trusted proxy, and `ip`, `ips`, `protocol`, `secure` and `hostname` follow the proxy.

## `/info`

Returns a JSON document with what the server is doing right now. It takes no parameters and doesn't use up any budget, so you can poll it while you test, for example with `watch -n1 curl -s http://127.0.0.1:3000/info`.

```json
{
  "name": "torchio",
  "version": "4.0.0",
  "uptimeSeconds": 321,
  "auth": false,
  "stress": { "workers": 4, "inFlight": 2, "perWorker": [1, 1, 0, 0], "burn": null },
  "delay": { "inFlight": 3 },
  "memory": { "heldMb": 200, "limitMb": 512, "processRssMb": 310, "containerLimitMb": 256 },
  "disk": { "heldMb": 100, "limitMb": 1024, "dir": "/tmp", "freeMb": 8120 },
  "fd": { "held": 0, "limit": 10000 },
  "ready": { "ready": true, "secondsLeft": null },
  "log": null,
  "limits": { "maxNumber": 50, "maxDelayMs": 60000, "maxHoldSeconds": 300, "maxLogLines": 100000, "maxLogRate": 10000 }
}
```

| Field                         | Meaning                                                                 |
|-------------------------------|-------------------------------------------------------------------------|
| `stress.inFlight`, `perWorker`| `/stress` requests being calculated, in total and for each worker       |
| `stress.burn`                 | What `/cpu` is doing, `{ "percent", "secondsLeft", "workers" }` (`workers` is how many are loaded), or `null` when it is not running |
| `delay.inFlight`              | `/delay` requests still waiting                                         |
| `memory.heldMb`, `limitMb`    | Memory held by `/memory` and the total it may hold (`MAX_MEMORY_MB`); the difference is what a new request can still allocate |
| `memory.processRssMb`         | Memory the whole process uses, the figure that counts for an OOM kill   |
| `memory.containerLimitMb`     | Memory limit of the container, `null` when there is none                |
| `disk.heldMb`, `limitMb`      | Same as for memory, for `/disk` (`MAX_DISK_MB`)                         |
| `disk.freeMb`                 | Free space in `DISK_DIR`, `null` if the directory is not usable         |
| `fd.held`, `limit`            | File descriptors held by `/fd` and the total it may hold (`MAX_FD`)     |
| `connections.held`, `limit`   | Connections held by `/connections` and the total it may hold (`MAX_CONNECTIONS`) |
| `log`                         | What `/log` is writing, `{ "sent", "lines", "rate" }`, or `null` when it is not running |
| `shutdown`                    | `{ "draining", "graceSeconds", "drainSeconds", "openRequests", "openConnections" }`: whether SIGTERM started a [drain](shutdown.md#shutdown), for how long (`null` if not), and what is open now, the request for `/info` and its connection included |
| `block`                       | `{ "count", "lastMs", "msLeft" }`: the blocks of `/block` so far, the length of the last one and what is left of the one running |
| `ready.ready`, `secondsLeft`  | What `/ready` answers, and the seconds left before it is ready again (`null` if nothing is counting down) |

## `/metrics`

The figures of [`/info`](#info) in the Prometheus text format, plus a counter of the requests answered and a histogram of the time they took. It takes no parameters and doesn't use up any budget.

| Metric                                      | Type    | Meaning                                                     |
|---------------------------------------------|---------|-------------------------------------------------------------|
| `torchio_info{version}`                 | gauge   | Always `1`, the version is in the label                     |
| `torchio_start_time_seconds`            | gauge   | Start time, in Unix time                                    |
| `torchio_workers`                       | gauge   | Worker threads for `/stress`                                |
| `torchio_stress_requests_in_flight`     | gauge   | `/stress` requests being calculated                         |
| `torchio_worker_requests_in_flight{worker}` | gauge | The same, for each worker                                 |
| `torchio_cpu_burn_percent`              | gauge   | Share of the time each worker is kept busy by `/cpu`, `0` when it is not running |
| `torchio_cpu_burn_workers`              | gauge   | Workers `/cpu` keeps busy, `0` when it is not running       |
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
| `torchio_connections_held`              | gauge   | TCP connections held by `/connections`                      |
| `torchio_connections_limit`             | gauge   | Total connections `/connections` may hold (`MAX_CONNECTIONS`) |
| `torchio_ready`                         | gauge   | `1` while `/ready` answers `200`, `0` while it answers `503` |
| `torchio_draining`                      | gauge   | `1` after SIGTERM while the server [drains](shutdown.md#shutdown)      |
| `torchio_open_requests`                 | gauge   | Requests being answered, the one for `/metrics` included    |
| `torchio_open_connections`              | gauge   | Connections open, the one for `/metrics` included           |
| `torchio_http_requests_total{route,code}` | counter | Requests answered, by route pattern (such as `/status/:code`) and status code. A request the client abandons before the answer is not counted |
| `torchio_http_request_duration_seconds{route,code}` | histogram | Time taken to answer, in seconds, with the same labels (`_bucket{le}`, `_sum`, `_count`). Buckets go from 5 ms to 60 s, which is the default `MAX_DELAY_MS`. It includes the time of `/delay`, and the requests to `/metrics` and `/info` themselves |

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
- `histogram_quantile(0.95, sum by (le, route) (rate(torchio_http_request_duration_seconds_bucket[5m])))` is the 95th percentile of the response time of each route.

## `/help`

Lists every endpoint with what it does, whether `TOKEN` protects it and its parameters with the default and the allowed range, so you can use the image without reading this page. It answers JSON, or plain text when asked by a browser:

```
curl -s localhost:3000/help | jq '.endpoints[] | select(.path == "/cpu")'
```

```json
{
  "path": "/cpu",
  "auth": true,
  "params": {
    "percent": { "fallback": 50, "min": 0, "max": 100 },
    "hold": { "fallback": 10, "min": 0, "max": 300 }
  },
  "description": "Keeps the workers busy for a share of the time, percent=0 stops it"
}
```

The limits are the ones of this instance (`MAX_*`). The list is built from the same definitions the endpoints read their parameters from (`src/endpoints.rs`), so it can't go out of date. `/ui` is listed only with `UI=1`.

## `/config`

Returns the effective configuration as JSON: port, number of workers, defaults and limits, `DISK_DIR`, `UI`, `START_DELAY_SECONDS`. The token is never shown, only whether there is one (`auth`). Like `/info`, it is always open.
