# Control panel

`torchio` also has a page in the browser that does the job of `watch` and lets you start the tests with sliders and buttons instead of `curl`. It is off by default, turn it on with `UI=1` and open `/ui`:

```
docker run -p 3000:3000 -e UI=1 altrdev/torchio:latest
```

![The torchio control panel: gauges with history for workers, memory, process RSS, disk, descriptors and requests, and a card for each test](images/ui.png)

The gauges and their history (the last 60 seconds) come from [`/info`](endpoints.md#info) and [`/metrics`](endpoints.md#metrics), refreshed every second, and the page shows whether `/ready` is answering `200` or `503`. Each card makes the request you see written under its sliders, and the answer goes in the log at the bottom, so it is also a way to learn the API. The history lives in the page: reloading it starts again.

Three cards do more than send one request. **Status** asks for any HTTP status. **Flaky** sends a batch of 20 requests to [`/flaky`](endpoints.md#flaky) and counts how many failed, since the failures come from the requests and not from the server. **Payload** downloads or uploads a body (generated, or a file you pick) and reports the speed, and if the body is cut short before its `Content-Length`: the browser counts what it gets and what it sends, so what is measured is the road between the browser and the server, proxies and ingresses included. An upload from the page is at most 200 MB, because the browser keeps the body in memory; the server's limit is `MAX_PAYLOAD_MB`. [`/oom`](endpoints.md#oom) has no card on purpose.

A card runs a [scenario](scenarios.md): you write the steps in JSON, start it and stop it, and the step it is in and the time left show in the card and at the top of the page. The page makes [`/crash`](endpoints.md#crash) a button, which is why it is off by default (and the button asks to be clicked twice). It needs no extra dependency and no build, it is one static file.
