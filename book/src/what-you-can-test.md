# What you can test

## Memory limits and OOM kills

Give the container a memory limit and ask for more than that:

```
docker run -d --name oom -m 256m --memory-swap 256m -e MAX_MEMORY_MB=1024 -p 3000:3000 altrdev/torchio
curl "http://127.0.0.1:3000/memory?mb=400"
docker inspect -f '{{.State.OOMKilled}}' oom
```

The container should be killed, and the last command prints `true`. `MAX_MEMORY_MB` is raised above the limit on purpose: its default (`512`) is a safety cap, so with it you can't exhaust a larger container. `--memory-swap` has the same value as `-m` so the container can't spill into swap, which on a host that has some (Docker Desktop does) would delay the kill or avoid it.

## Autoscaling

A core stays busy only while a request is running, so send at least as many concurrent requests as workers (one per CPU by default):

```
hey -z 60s -c 8 "http://127.0.0.1:3000/stress?number=40"
```

To hold the CPU at a level for a while, without sending requests, use `/cpu`. A CPU based autoscaler reacts to a percentage, which this gives you directly:

```
curl "http://127.0.0.1:3000/cpu?percent=80&hold=300"
```

More about it in [Load testing](load-testing.md#load-testing).

## Probes, timeouts and retries

```
curl -i http://127.0.0.1:3000/status/503
curl --max-time 2 "http://127.0.0.1:3000/delay?ms=5000"
curl -i "http://127.0.0.1:3000/flaky?rate=30"
```

`/status/:code` answers with any status from `200` to `599`, and `/delay` waits without blocking the server, so the rest keeps being served while a slow request is pending. `/flaky` fails only some of the requests, to see how retries, circuit breakers and outlier detection behave.

## Disk and volumes

```
curl "http://127.0.0.1:3000/disk?mb=500&hold=60"
```

If the write fails (disk full, read-only filesystem, directory not writable) the answer is a `507`. Point `DISK_DIR` to a mounted volume to test it.

## Leaks and slow starts

```
curl "http://127.0.0.1:3000/leak?rate=10&mb=400&hold=60"
docker run -p 3000:3000 -e START_DELAY_SECONDS=30 altrdev/torchio
```

`/leak` grows the memory by itself, a few MB each second, so you can watch a memory alert or an autoscaler react before the OOM kill. `START_DELAY_SECONDS` keeps the port closed for a while after the start, to test startup probes, rollouts and `depends_on`.

## Readiness, restarts and descriptors

```
curl "http://127.0.0.1:3000/ready/off?seconds=60"
curl -i http://127.0.0.1:3000/ready
curl "http://127.0.0.1:3000/crash?code=1"
curl "http://127.0.0.1:3000/block?ms=10000"
curl "http://127.0.0.1:3000/fd?count=500&hold=30"
```

`/ready/off` makes `/ready` answer `503` for a while while `/` keeps answering `200`, so you can see an instance leave a Service or a load balancer without being restarted. `/crash` ends the process with the exit code you choose, to watch restarts and backoff. `/block` freezes every thread of the server for a while, so a liveness probe times out on an instance that is alive. `/oom` (with `ALLOW_OOM=1`) allocates until the kernel kills the container, to see `OOMKilled` and the exit code `137`. `/fd` holds file descriptors open, to see what a process does when it runs out of them.

## Metrics and dashboards

[`/info`](endpoints.md#info) is a quick look at what the server holds and what is left of each budget, and [`/metrics`](endpoints.md#metrics) has the same figures for Prometheus.
