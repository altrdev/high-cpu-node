# Shutdown

By default SIGTERM and SIGINT end the process right away, without waiting for the requests in progress, and release what the server holds (the files of `/disk`). To see what an orchestrator or a load balancer does during a rollout, set `SHUTDOWN_GRACE_SECONDS`: the server then **drains** first.

- It closes the port, so a new connection is refused, and keeps serving the requests and the connections that are already open.
- It writes what is going on every second: `Draining, 3 s: 2 requests and 5 connections open`.
- It ends when nothing is open (`Drained after 2.4 s`) or when the grace period is over, whichever comes first, and releases what it holds then. A second signal ends it at once.
- While it drains, [`/info`](endpoints.md#info) (`shutdown`) and [`/metrics`](endpoints.md#metrics) (`torchio_draining`, `torchio_open_requests`, `torchio_open_connections`) say so, for a client that still has a connection open: it is served as before, and a connection that stays idle keeps the drain waiting.

```
docker run -p 3000:3000 -e SHUTDOWN_GRACE_SECONDS=30 altrdev/torchio
```

[`/delay`](endpoints.md#delay) and [`/connections`](endpoints.md#connections) make something to wait for: `/delay?ms=10000` is a request in progress, and `/connections?count=50&hold=60` leaves 50 connections open that the drain waits for until the grace period is over. In Kubernetes, `terminationGracePeriodSeconds` must be longer than `SHUTDOWN_GRACE_SECONDS`, or the kubelet ends the container first.
