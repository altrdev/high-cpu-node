---
name: torchio
description: Put a container or service under controlled pressure with torchio, a small Rust server that burns CPU, holds memory, fills disk, answers slowly or fails on demand over HTTP. Use it to test autoscaling, resource limits, OOM kills, probes, timeouts, retries and restarts, or when the user mentions torchio.
---

# torchio

torchio is a server you run in the thing under test (a container, a pod, a VM). You ask it over HTTP, or with the `torchio` command, to consume a resource or to misbehave, and you watch how the platform reacts. Full documentation: https://torchio.altr.dev, all in one file at https://torchio.altr.dev/llms-full.txt.

## Before you start

- The server must be running where the test is meant to happen: `docker run -p 3000:3000 altrdev/torchio`, or `torchio serve`. The port is `3000` (`PORT`).
- Check what you are talking to: `curl -s localhost:3000/info` is what it holds right now and what is left of each budget, and `curl -s localhost:3000/help` lists every endpoint with its parameters, defaults and limits. Use these instead of remembering numbers: the limits come from the server's own settings (`MAX_MEMORY_MB`, `MAX_DISK_MB`, `MAX_HOLD_SECONDS`, ...).
- If `/info` says a token is needed, the endpoints that change state want `Authorization: Bearer <token>` (or `--token` / `TOKEN` with the command). Ask the user for it; never guess one.
- It consumes resources on purpose. Run it only in environments the user controls, never on a shared or production service unless they say so.

## Ask before you run these

`/crash` (ends the process), `/oom` (allocates until the kernel kills it, needs `ALLOW_OOM=1`), `/block` (freezes the whole server, `/info` included) and `/ready/off` on something real. They are the point of some tests, but they take the instance away. Say what will happen and wait for a yes.

## The endpoints

Each one that changes state is also a command: `torchio <name> [parameters]`, same names as the query parameters, in the order of the reference.

| To test | Request | Command |
|---|---|---|
| CPU load at a level (autoscaling) | `/cpu?percent=80&hold=300` | `torchio cpu 80 --hold 300` |
| CPU on concurrent requests | `/stress?number=40` (needs about one concurrent request per worker) | `torchio stress 40` |
| Memory, OOM kills | `/memory?mb=400&hold=60` | `torchio memory 400 --hold 60` |
| A memory leak | `/leak?rate=10&mb=400&hold=60` | `torchio leak --rate 10 --mb 400` |
| Full disk, volumes | `/disk?mb=500&hold=60` | `torchio disk 500 --hold 60` |
| Out of descriptors / connections | `/fd?count=500&hold=30`, `/connections?count=100` | `torchio fd 500` |
| Slow answers, timeouts | `/delay?ms=5000` | `torchio delay 5000` |
| Any status, retries, circuit breakers | `/status/503`, `/flaky?rate=30&code=503` | `torchio status 503` |
| Readiness | `/ready/off?seconds=60`, `/ready` | `torchio ready off 60` |
| Restarts and backoff | `/crash?code=1` | `torchio crash 1` |
| Liveness probe timing out | `/block?ms=10000` | `torchio block 10000` |
| Body size limits of proxies | `/payload?mb=100`, `POST /payload` | `torchio payload get 100` |
| Logging volume | `/log?lines=1000&rate=100` | `torchio log 1000` |

Notes that save a wrong turn:

- `hold` is in seconds; memory, disk, descriptors and connections go back when it ends. `hold=0` releases at once.
- The budgets are shared by everyone: a request that would go over gets `429` (memory, disk, fd, connections) or `507` (the disk write failed). That is the server's limit, not a bug. `/info` says what is left.
- For an OOM test the limit of the container must be below `MAX_MEMORY_MB`, whose default (`512`) is a safety cap. Raise `MAX_MEMORY_MB` on purpose, and set `--memory-swap` equal to `-m`.
- A CPU level only holds while it is asked: `/cpu?percent=0` stops it.

## Several steps: a scenario

For a test with more than one phase, write it once and let the server run it:

```
torchio scenario ramp.json --wait     # exits 0 only if the scenario finished
```

where `ramp.json` is a list of steps, each with one action and `seconds`:

```
[{"cpu": 30, "seconds": 120}, {"cpu": 90, "seconds": 300}, {"cpu": 0, "seconds": 300}, {"memory": 400, "seconds": 60}, {"ready": "off", "seconds": 30}]
```

Actions: `cpu` (with optional `workers`), `memory`, `disk`, `fd`, `connections`, `ready` (`"off"`), `block` (takes no `seconds`), or none for a pause. The whole list is checked before it starts and a mistake is a `400` naming the step. One scenario runs at a time (`409`); `GET /scenario` shows where it is and `DELETE /scenario` stops it.

## Watching the result

- `torchio watch` (or `torchio watch --once` for one reading) shows workers, memory, disk, descriptors, what is held and the seconds left.
- `curl -s localhost:3000/info` for scripts; `/metrics` for Prometheus (`torchio_*`).
- The effect on the platform is what you are testing: look at it where it happens (`kubectl get pods -w`, `kubectl top`, `docker inspect -f '{{.State.OOMKilled}}'`, the autoscaler's events), not only in torchio.

## When you are done

Everything held is released by itself when its `hold` ends. To end earlier: `/cpu?percent=0`, `/ready/on`, `DELETE /scenario`, or stop the container.
