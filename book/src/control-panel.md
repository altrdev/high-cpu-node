# Control panel

`torchio` also has a page in the browser that does the job of `watch` and lets you start the tests with sliders and buttons instead of `curl`. It is off by default, turn it on with `UI=1` and open `/ui`:

```
docker run -p 3000:3000 -e UI=1 altrdev/torchio:latest
```

![The torchio control panel: gauges with history for workers, memory, process RSS, disk, descriptors and requests, and a card for each test](https://raw.githubusercontent.com/altrdev/torchio/refs/heads/main/docs/ui.png)

The gauges and their history (the last 60 seconds) come from [`/info`](endpoints.md#info) and [`/metrics`](endpoints.md#metrics), refreshed every second, and the page shows whether `/ready` is answering `200` or `503`. Each card makes the request you see written under its sliders, and the answer goes in the log at the bottom, so it is also a way to learn the API. The history lives in the page: reloading it starts again.

A card runs a [scenario](scenarios.md): you write the steps in JSON, start it and stop it, and the step it is in and the time left show in the card and at the top of the page. The page makes [`/crash`](endpoints.md#crash) a button, which is why it is off by default (and the button asks to be clicked twice). It needs no extra dependency and no build, it is one static file.
