# torchio

[![CI](https://github.com/altrdev/torchio/actions/workflows/ci.yml/badge.svg)](https://github.com/altrdev/torchio/actions/workflows/ci.yml) [![Docker image](https://img.shields.io/docker/v/altrdev/torchio.svg?sort=semver&label=docker%20image)](https://hub.docker.com/r/altrdev/torchio) [![Docker pulls](https://img.shields.io/docker/pulls/altrdev/torchio.svg)](https://hub.docker.com/r/altrdev/torchio) [![License](https://img.shields.io/github/license/altrdev/torchio)](https://github.com/altrdev/torchio/blob/main/LICENSE)

**Break your containers on purpose.** torchio is a small server, a single static binary written in Rust, that burns CPU, holds memory, fills disk, answers slowly or returns any HTTP status, all on demand over HTTP. Use it to see how your setup behaves under pressure: autoscaling, resource limits, OOM kills, probes, timeouts and retries.

*Torchio* is the Italian word for the press that squeezes wine and oil, as in *mettere sotto torchio*: to put under pressure. Put your containers under the press. It began in 2019 as `high-cpu-node`, when it could only burn CPU.

![torchio watch: gradient bars and history of busy workers, held memory, process RSS and held disk, while keys start a memory test, a CPU burn and a leak](https://raw.githubusercontent.com/altrdev/torchio/refs/heads/main/docs/watch.svg)

Source code and issues: https://github.com/altrdev/torchio

> **Warning:** this server is meant to consume resources on purpose. Don't expose it to the public internet.

## Try it in 30 seconds

Start the server with Docker:

```
docker run --name torchio -p 3000:3000 altrdev/torchio
```

Or on your local machine, with Homebrew (macOS and Linux):

```
brew install altrdev/tap/torchio
torchio serve
```

`torchio` with no command does the same as `torchio serve`: it is what the Docker image runs. The server stays in the foreground, so Ctrl+C stops it; use a second terminal tab for `torchio watch`.

Other ways to get the `torchio` command. A script that downloads the binary of the latest release for your system (macOS or Linux, `amd64` or `arm64`), checks its checksum and installs it:

```
curl -fsSL https://raw.githubusercontent.com/altrdev/torchio/main/install.sh | sh
```

It installs in `/usr/local/bin` (with `sudo` if needed, or in `~/.local/bin` when there is none). `TORCHIO_VERSION=v4.0.0` picks a release and `TORCHIO_INSTALL_DIR` the folder, for example `curl -fsSL ... | TORCHIO_INSTALL_DIR=$HOME/bin sh`. Or take the binary by hand from the [releases](https://github.com/altrdev/torchio/releases), or build it from the source (Rust 1.88 or later):

```
cargo install --git https://github.com/altrdev/torchio
```

In a second terminal, watch what it does:

```
docker exec -it torchio torchio watch    # with Docker
torchio watch                            # on your local machine, press m to start a memory test
```

In a third one, put it under load, and see the bars move:

```
torchio memory 200 --hold 20
torchio disk 100 --hold 20
torchio stress 44
```

The same thing with `curl`: `curl "http://127.0.0.1:3000/memory?mb=200&hold=20"`. See [Starting a test](#starting-a-test).

## What you can test

### Memory limits and OOM kills

Give the container a memory limit and ask for more than that:

```
docker run -d --name oom -m 256m --memory-swap 256m -e MAX_MEMORY_MB=1024 -p 3000:3000 altrdev/torchio
curl "http://127.0.0.1:3000/memory?mb=400"
docker inspect -f '{{.State.OOMKilled}}' oom
```

The container should be killed, and the last command prints `true`. `MAX_MEMORY_MB` is raised above the limit on purpose: its default (`512`) is a safety cap, so with it you can't exhaust a larger container. `--memory-swap` has the same value as `-m` so the container can't spill into swap, which on a host that has some (Docker Desktop does) would delay the kill or avoid it.

### Autoscaling

A core stays busy only while a request is running, so send at least as many concurrent requests as workers (one per CPU by default):

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
docker run -p 3000:3000 -e START_DELAY_SECONDS=30 altrdev/torchio
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
| `/log`          | `lines`, `rate`    | Writes lines to stdout at a rate, to test log collection and rotation |
| `/echo`         |                    | The request as the server sees it, to see what proxies add or remove |
| `/ui`           |                    | Control panel in the browser, only with `UI=1`              |
| `/info`         |                    | JSON with what the server is doing and the remaining budgets |
| `/metrics`      |                    | The same figures in the Prometheus text format               |
| `/help`         |                    | Every endpoint with its parameters, defaults and limits      |
| `/config`       |                    | The effective configuration, without the token               |

Every query parameter is optional. A value that is not a number falls back to its default; a number outside the allowed range gets a `400`. All the limits are configurable, see [Configuration](#configuration).

### `/stress`

Runs a recursive Fibonacci calculation in a worker thread, so it keeps one core busy for the whole request.

| Parameter | Default | Allowed range      |
|-----------|---------|--------------------|
| `number`  | `45`    | `0..MAX_NUMBER`    |

The calculation time grows exponentially with `number`.

```
curl "http://127.0.0.1:3000/stress?number=40"
```

### `/cpu`

Use [`/stress`](#stress) when the load must come from requests, so that response times show the pressure and concurrent requests queue up; use `/cpu` when you only need the CPU held at a level, as a percentage, for a while.

Keeps every worker busy for `percent` of the time during `hold` seconds, instead of running one calculation. With one worker for each CPU (the default), `percent` is the load of the machine, which is what a CPU based autoscaler looks at. A new request replaces the previous one, and `percent=0` stops it.

| Parameter | Default | Allowed range         |
|-----------|---------|-----------------------|
| `percent` | `50`    | `0..100`              |
| `hold`    | `10`    | `0..MAX_HOLD_SECONDS` |

The answer comes right away. `/stress` keeps working while `/cpu` runs, and what is left is in `/info` (`stress.burn`) and in `/metrics` (`torchio_cpu_burn_percent`). The default number of workers is the number of CPUs the process can use, so a CPU limit of the container counts (a limit of 2 CPUs gives 2 workers); set `WORKERS` to choose another.

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

Every page is written, so the memory counts as used (RSS) as soon as the request is answered, and it goes back to the OS when it is released. The server itself needs only a few MB on top of what you allocate.

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

### `/log`

Writes `lines` lines (default 1000) to stdout at `rate` lines each second (default 100), then stops, to test log collection pipelines, rotation and how fast the logs of a container fill a disk. Each line has a timestamp and its number: `2026-10-18T14:00:00.000Z torchio-log line 7 of 1000`. A new request replaces the one still running; the lines already written stay. `MAX_LOG_LINES` and `MAX_LOG_RATE` set the limits. While it writes, [`/info`](#info) has `log` (`{ "sent", "lines", "rate" }`, `null` otherwise), and `watch` and the control panel show it.

### `/echo`

Returns the request as this server sees it: method, URL, protocol, `hostname`, client address (`ip`, the `ips` chain and `remoteAddress`, the peer of the connection) and all the headers. Call it through an ingress controller, a proxy or a load balancer to see what they add, remove or rewrite (`Host`, `X-Forwarded-For`, `X-Forwarded-Proto`). `Authorization`, `Proxy-Authorization` and `Cookie` are shown as `[masked]`.

By default the `X-Forwarded-*` headers are not trusted: `ip` is the peer and `protocol` is `http`. Set `TRUST_PROXY` to let them decide: `true` trusts every proxy, a number trusts that many hops from the server, and a list of addresses, subnets (`10.0.0.0/8`) and the names `loopback`, `linklocal` and `uniquelocal` trusts those. The client is then the first address, walking back from the connection through `X-Forwarded-For`, that is not a trusted proxy, and `ip`, `ips`, `protocol`, `secure` and `hostname` follow the proxy.

### `/info`

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
| `stress.burn`                 | What `/cpu` is doing, `{ "percent", "secondsLeft" }`, or `null` when it is not running |
| `delay.inFlight`              | `/delay` requests still waiting                                         |
| `memory.heldMb`, `limitMb`    | Memory held by `/memory` and the total it may hold (`MAX_MEMORY_MB`); the difference is what a new request can still allocate |
| `memory.processRssMb`         | Memory the whole process uses, the figure that counts for an OOM kill   |
| `memory.containerLimitMb`     | Memory limit of the container, `null` when there is none                |
| `disk.heldMb`, `limitMb`      | Same as for memory, for `/disk` (`MAX_DISK_MB`)                         |
| `disk.freeMb`                 | Free space in `DISK_DIR`, `null` if the directory is not usable         |
| `fd.held`, `limit`            | File descriptors held by `/fd` and the total it may hold (`MAX_FD`)     |
| `log`                         | What `/log` is writing, `{ "sent", "lines", "rate" }`, or `null` when it is not running |
| `ready.ready`, `secondsLeft`  | What `/ready` answers, and the seconds left before it is ready again (`null` if nothing is counting down) |

### `/metrics`

The figures of [`/info`](#info) in the Prometheus text format, plus a counter of the requests answered and a histogram of the time they took. It takes no parameters and doesn't use up any budget.

| Metric                                      | Type    | Meaning                                                     |
|---------------------------------------------|---------|-------------------------------------------------------------|
| `torchio_info{version}`                 | gauge   | Always `1`, the version is in the label                     |
| `torchio_start_time_seconds`            | gauge   | Start time, in Unix time                                    |
| `torchio_workers`                       | gauge   | Worker threads for `/stress`                                |
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

### `/help`

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

### `/config`

Returns the effective configuration as JSON: port, number of workers, defaults and limits, `DISK_DIR`, `UI`, `START_DELAY_SECONDS`. The token is never shown, only whether there is one (`auth`). Like `/info`, it is always open.

## Starting a test

Every endpoint that changes the state of the server is also a command, so you don't need `curl`. It calls a running server (`--url`, by default `http://127.0.0.1:$PORT` or port `3000`) and prints its answer:

```
torchio cpu 80 --hold 60          # 80% of every worker for a minute
torchio memory 512                # hold 512 MB for the default time
torchio leak --rate 5 --mb 1000
torchio ready off 30              # not ready for 30 seconds
torchio status 503
torchio crash 137 --after 5
```

| Command                          | Endpoint                 |
|----------------------------------|--------------------------|
| `torchio stress [number]`        | [`/stress`](#stress)     |
| `torchio cpu [percent] [hold]`   | [`/cpu`](#cpu)           |
| `torchio delay [ms]`             | [`/delay`](#delay)       |
| `torchio status <code>`          | [`/status/:code`](#statuscode) |
| `torchio memory [mb] [hold]`     | [`/memory`](#memory)     |
| `torchio leak [rate] [mb] [hold]`| [`/leak`](#leak)         |
| `torchio disk [mb] [hold]`       | [`/disk`](#disk)         |
| `torchio fd [count] [hold]`      | [`/fd`](#fd)             |
| `torchio log [lines] [rate]`     | [`/log`](#log)           |
| `torchio crash [code] [after]`   | [`/crash`](#crash)       |
| `torchio ready on \| off [seconds]` | [`/ready`](#ready)   |

The values after the name are the parameters of the endpoint, in the order of the table; any of them can also be given by name, as `--hold 60` or `--hold=60`. The limits are the ones of the server: a value it refuses is printed with its `400` and the command exits with `1`. With [`TOKEN`](#token), pass `--token <token>` or set `TOKEN`. `/stress` and `/delay` answer when they finish, so the command waits for them. Like `watch`, it speaks plain HTTP only.

## Shell completion

`torchio completions <bash|zsh|fish>` prints a script that makes the shell complete the commands (`torchio me<Tab>` gives `memory`), their options (`--hold`, `--url`…) and the words `on` and `off` of `ready`. With Homebrew the completions are installed with the binary: open a new shell. Otherwise:

```
# bash: in ~/.bashrc
eval "$(torchio completions bash)"

# zsh: in ~/.zshrc, after compinit
eval "$(torchio completions zsh)"

# fish
torchio completions fish > ~/.config/fish/completions/torchio.fish
```

If Tab does nothing in zsh after `brew install`, there are two usual causes:

- The folder of the completions, `$(brew --prefix)/share/zsh/site-functions`, is not in your `fpath`. `brew shellenv` adds it, so run it before `compinit` or before Oh My Zsh (which calls `compinit`): `eval "$(brew shellenv)"`. See [Homebrew's page on shell completion](https://docs.brew.sh/Shell-Completion).
- `compinit` ignores the folders that the group can write to, and Oh My Zsh calls it with `-i`, so it does not say so. If `ls -ld $(brew --prefix)/share/zsh/site-functions` shows `drwxrwxr-x`, remove the group's write permission from that folder and its two parents, and not from the whole prefix: `chmod go-w "$(brew --prefix)/share" "$(brew --prefix)/share/zsh" "$(brew --prefix)/share/zsh/site-functions"`. Then open a new shell, and delete `~/.zcompdump*` if it still does not complete.

The scripts are generated from the commands of the binary, so they always match the version you have. On the command line of [`watch`](#keys), Tab completes the name of a command.

## Watching a server

`torchio watch` shows what a server is doing in the terminal, refreshing every second with bars and a short history of the last samples (workers, memory, process memory, disk and file descriptors), plus whether `/ready` is answering `200` or `503`. It is a command of the same binary, reads [`/info`](#info) and works against any running instance:

```
torchio watch
torchio watch --url http://my-host:3000 --interval 2
```

Inside a container, where the server is on `localhost`:

```
docker exec -it <container> torchio watch
```

| Option        | Default                    | Description                                              |
|---------------|----------------------------|----------------------------------------------------------|
| `--url`       | `http://127.0.0.1:$PORT` or port `3000` | Server to watch                              |
| `--interval`  | `1`                        | Seconds between samples                                  |
| `--token`     | `$TOKEN`                   | Token for the commands that need it                      |
| `--read-only` |                            | Only look: no keys, even on a terminal                   |
| `--once`      |                            | Print one sample and exit; the status is `1` if the server can't be reached |

### Keys

On a terminal, `watch` also starts the tests, so you can load a server and see the bars move in the same window. A key opens a command line with the command already written, you add the values and press Enter:

| Key | Command | Key | Command |
|-----|---------|-----|---------|
| `c` | `cpu`   | `g` | `log`   |
| `m` | `memory`| `s` | `stress`|
| `l` | `leak`  | `r` | `ready` |
| `d` | `disk`  | `x` | `crash` |
| `f` | `fd`    | `:` | any command, like `status 503` |

The line shows the parameters of the command while you type (`cpu [percent] [hold]`), Enter with only the name uses the defaults, Esc cancels and `q` or Ctrl+C quits. The answer of the server, or its error, stays under the keys. The request runs in the background, so the view keeps refreshing while `/stress` or `/delay` wait. The commands are the ones of [Starting a test](#starting-a-test).

The keys are on only when both the input and the output are a terminal. `--read-only` turns them off, which is the way to watch a shared instance without a key that can [`/crash`](#crash) it. With `--once`, or with the output in a pipe or a file, `watch` never takes keys.

It speaks plain HTTP: to watch a server behind HTTPS, reach it with a port-forward. A server older than 3.1 doesn't report descriptors and readiness, so those two lines are left out. The history only covers the time the command has been running. On a terminal the bars are smooth and shift from green to red as they get close to their limit (blue where there is no limit to compare with), the history is brighter when it is higher, and what is happening (`/stress`, `/cpu`, `/log`) lights up while the rest stays dim. Readiness is a badge, green or red. Set `NO_COLOR=1` for plain text, which is also what you get when the output is not a terminal.

## Control panel

`torchio` also has a page in the browser that does the job of `watch` and lets you start the tests with sliders and buttons instead of `curl`. It is off by default, turn it on with `UI=1` and open `/ui`:

```
docker run -p 3000:3000 -e UI=1 altrdev/torchio:latest
```

![The torchio control panel: gauges with history for workers, memory, process RSS, disk, descriptors and requests, and a card for each test](https://raw.githubusercontent.com/altrdev/torchio/refs/heads/main/docs/ui.png)

The gauges and their history (the last 60 seconds) come from [`/info`](#info) and [`/metrics`](#metrics), refreshed every second, and the page shows whether `/ready` is answering `200` or `503`. Each card makes the request you see written under its sliders, and the answer goes in the log at the bottom, so it is also a way to learn the API. The history lives in the page: reloading it starts again.

The page makes [`/crash`](#crash) a button, which is why it is off by default (and the button asks to be clicked twice). It needs no extra dependency and no build, it is one static file.

## Token

Anyone who can reach the port can call `/crash` or `/disk`. Set `TOKEN` to ask for a password on the endpoints that change the state of the instance (`/stress`, `/cpu`, `/memory`, `/leak`, `/disk`, `/fd`, `/log`, `/ready/off`, `/ready/on` and `/crash`):

```
docker run -p 3000:3000 -e TOKEN=secret altrdev/torchio:latest
curl -H "Authorization: Bearer secret" localhost:3000/crash
```

Without the header, or with a wrong token, they answer `401`. `/`, `/ready`, `/info`, `/metrics`, `/help`, `/config`, `/echo`, `/delay` and `/status/:code` stay open, so probes, Prometheus and `watch` work as before. [`/info`](#info) tells whether a token is needed (`auth`). The control panel asks for it the first time a server answers `401` and keeps it for that tab. Without `TOKEN` nothing is checked.

It is a shared password, not a login: without HTTPS it travels in clear, so on the internet put a proxy with TLS in front.

## Configuration

Environment variables:

| Variable           | Default         | Description                                       |
|--------------------|-----------------|---------------------------------------------------|
| `PORT`             | `3000`          | Port the server listens on                        |
| `WORKERS`          | CPUs of the process | Number of worker threads for `/stress` and `/cpu` |
| `MAX_NUMBER`       | `50`            | Highest `number` accepted by `/stress`            |
| `MAX_DELAY_MS`     | `60000`         | Highest `ms` accepted by `/delay`                 |
| `MAX_MEMORY_MB`    | `512`           | Total memory `/memory` can hold at once           |
| `MAX_DISK_MB`      | `1024`          | Total disk space `/disk` can hold at once         |
| `UI`               | off             | `1` turns on the control panel at `/ui`           |
| `START_DELAY_SECONDS` | `0`         | Seconds the port stays closed after the start     |
| `MAX_FD`           | `10000`         | Total descriptors `/fd` can hold at once          |
| `MAX_LOG_LINES`    | `100000`        | Highest `lines` accepted by `/log`                |
| `MAX_LOG_RATE`     | `10000`         | Highest `rate` accepted by `/log`                 |
| `TRUST_PROXY`      | off             | Trusts `X-Forwarded-*` (`true`, a number of hops, or addresses and subnets), see `/echo` |
| `TOKEN`            | none            | Makes the endpoints that change the state need `Authorization: Bearer <token>` |
| `MAX_HOLD_SECONDS` | `300`           | Highest `hold`, `seconds` and `after` accepted by the endpoints that take them |
| `DISK_DIR`         | system temp dir | Directory where `/disk` writes its files          |

A number must be a positive integer. A value that is not one is reported at startup and replaced by the default.

```
docker run -p 3000:3000 -e WORKERS=2 -e MAX_MEMORY_MB=2048 altrdev/torchio:latest
```

## Docker

The image is built for `linux/amd64` and `linux/arm64`. It holds the static binary and an empty `/tmp`, about 1.4 MB in all, and nothing else: no shell and no package manager, so `docker exec` runs `torchio` itself (`torchio watch`) rather than a shell. It runs as the non-root user `65532` and has a `HEALTHCHECK` on `/`, which runs `torchio healthcheck`. With `START_DELAY_SECONDS` the container is unhealthy until the port opens, so give the healthcheck a longer `--start-period`. Because of the non-root user, a directory you use as `DISK_DIR` (for example a mounted volume) must be writable by it.

Each release is tagged `X.Y.Z` (the exact version), `X.Y` and `X` (the latest patch of the minor and of the major), `vX.Y.Z` and `latest`. `latest`, `X.Y` and `X` move with each release, so pin the exact version where a change must not come by itself. The [tags on Docker Hub](https://hub.docker.com/r/altrdev/torchio/tags) of the first releases only have the `v`.

The binary alone, for Linux and macOS on `amd64` and `arm64`, is attached to each [release](https://github.com/altrdev/torchio/releases) with its `SHA256SUMS`.

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

`/stress` requests are spread round-robin over a pool of worker threads (one for each CPU by default), so concurrent requests can saturate multiple cores. The other endpoints run on an async runtime (tokio), so a slow request never blocks the rest. The memory of `/memory` and `/leak` comes straight from the OS and goes back as soon as it is released.

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
cargo run               # starts the server
cargo run -- watch      # terminal view of a running server
cargo test              # every test: unit, router and integration
cargo clippy --all-targets -- -D warnings
cargo fmt
```

```
src/
  main.rs          the program: runs the server or one of the commands
  lib.rs           the crate, with a line on each module
  config.rs        environment variables, validated
  endpoints.rs     the parameters and limits of every endpoint, shared by the routes and /help
  app.rs           what the routes share, and the router
  server.rs        listens, and ends the process
  routes/          one file for each endpoint: reads the parameters, asks resources, writes the answer
  resources/       what the server holds or does on purpose, without knowing about HTTP
    budget.rs        the total that /memory, /disk and /fd share between requests
    memory.rs        memory taken from the OS (/memory, /leak)
    disk.rs          the files of /disk
    fd.rs            the descriptors of /fd
    workers.rs       the pool of worker threads for /stress and /cpu
    ready.rs         the readiness switch
    log.rs           the lines of /log
    delay.rs         the requests of /delay still waiting
  http/            what serves requests and is not specific to an endpoint
    params.rs        the integer query parameters
    auth.rs          TOKEN
    trust.rs         TRUST_PROXY
    accept.rs        JSON or text for /help
    counters.rs      requests answered, for /metrics
  cli/             the commands besides the server
    watch.rs         torchio watch
    view.rs          what watch draws
    info.rs          what /info says, as watch reads it
    http.rs          the small HTTP client of the commands
    healthcheck.rs   torchio healthcheck
  snapshot.rs      the figures behind /info and /metrics
  system.rs        RSS, the memory limit of the container, free space, names of errors
  units.rs         units of size
assets/ui.html     the control panel, a single static page
tests/             integration tests, by area
Dockerfile         builds the binary for both platforms with cargo-zigbuild, into an image of one file
```

The tests come in three kinds, and `cargo test` runs them all:

- **Unit tests** live in the module they test (`#[cfg(test)]`): the budgets, the parsing of parameters and `TRUST_PROXY`, the view of `watch`, the worker pool, the timers (with a paused clock).
- **Router tests** (`tests/api.rs`) call the router without a socket: every endpoint of the catalog is routed and protected as it says, every parameter is rejected one step outside its range, CORS, methods, content negotiation, and the shape of `/info`, `/config` and `/metrics`.
- **Integration tests** (`tests/*.rs`, with what they share in `tests/common`) start the real binary on a free port with small limits and call it over HTTP: `probes` (`/`, `/status`, `/ready`, `/delay`), `resources` (`/memory`, `/leak`, `/disk`, `/fd`), `load` (`/stress`, `/cpu`), `observability` (`/info`, `/metrics`, `/log`, `/echo`, `/help`, `/config`), `lifecycle` (configuration, start, signals, `/crash`), `auth` (`TOKEN`), `ui` and `cli` (`watch`, `healthcheck`).

CI runs them on Linux and macOS with the format check, `clippy` and the documentation, builds the image for both platforms, runs it with a memory limit and a CPU limit, and validates `/metrics` with `promtool`.

## Changelog

See [CHANGELOG.md](https://github.com/altrdev/torchio/blob/main/CHANGELOG.md) and the [GitHub releases](https://github.com/altrdev/torchio/releases).

## Security

This tool is meant to consume resources, so run it only in environments you control and don't expose it to the internet. If you find a security problem, please open an issue at https://github.com/altrdev/torchio/issues.

## License

MIT
