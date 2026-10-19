# Scenarios

A test is often more than one request: load the CPU for a minute, then fill the memory, then take the instance out of the Service. A scenario is that list written once, in JSON, and run by the server by itself, one step after the other. You can repeat it, keep it in the repository next to the manifests and run it from a pipeline (`torchio scenario file.json --wait`), without a script that calls the endpoints in order.

```
curl -X POST http://127.0.0.1:3000/scenario -d '[
  {"cpu": 80, "seconds": 60},
  {"memory": 500, "seconds": 30},
  {"ready": "off", "seconds": 20}
]'
```

The server answers `202` with the scenario and its first step already running. The three steps above take 110 seconds: the CPU at 80% for a minute, then 500 MB held for 30 seconds, then `/ready` answering `503` for 20.

## How a step is written

A step is an object with **one action** and **`seconds`**, how long it lasts. When the seconds are over the next step starts.

| Action        | Value                  | What it does                                                            | Allowed range           |
|---------------|------------------------|-------------------------------------------------------------------------|-------------------------|
| `cpu`         | percent                | Keeps the workers busy for that share of the time, like [`/cpu`](endpoints.md#cpu) | `0..100`        |
| `memory`      | MB                     | Holds that much memory, like [`/memory`](endpoints.md#memory)           | `1..MAX_MEMORY_MB`      |
| `disk`        | MB                     | Holds a file of that size in `DISK_DIR`, like [`/disk`](endpoints.md#disk) | `1..MAX_DISK_MB`     |
| `fd`          | count                  | Holds that many descriptors, like [`/fd`](endpoints.md#fd)              | `1..MAX_FD`             |
| `connections` | count                  | Holds that many connections to the server, like [`/connections`](endpoints.md#connections) | `1..MAX_CONNECTIONS` |
| `ready`       | `"off"`                | Makes `/ready` answer `503`, like [`/ready/off`](endpoints.md#ready) | only `"off"`            |
| `block`       | ms                     | Freezes the server, like [`/block`](endpoints.md#block)                 | `1..MAX_BLOCK_MS`       |
| *(none)*      |                        | A pause: nothing happens for `seconds`                                  |                         |

- **`seconds`** is needed in every step, from `1` to `MAX_HOLD_SECONDS`. The only exception is `block`, which lasts as long as its `ms` says (rounded up to the second) and takes no `seconds`.
- **`workers`** goes with `cpu` and loads only the first workers, from `1` to the number of workers. Without it every worker is loaded: `{"cpu": 80, "workers": 1, "seconds": 60}`.
- A scenario has at most **100 steps**.

The limits are the ones of the endpoints: a step can't ask for more than the endpoint would give. A scenario is made of what the server can already do, with the same budgets.

### What is not a step

- [`/crash`](endpoints.md#crash) and [`/oom`](endpoints.md#oom) end the instance, and nothing is left to run the next step.
- [`/delay`](endpoints.md#delay), [`/status/:code`](endpoints.md#statuscode) and [`/flaky`](endpoints.md#flaky) act on the requests that come, not on the server, so there is no time in them to put in a step. Run them from your client while the scenario goes.
- [`/leak`](endpoints.md#leak), [`/log`](endpoints.md#log) and [`/payload`](endpoints.md#payload) are not steps: they take several values or need a client on the other side.

## Checked before it starts

The whole scenario is checked when it arrives, before anything runs. A mistake is a `400` that names the step and nothing starts, so you never find out at minute ten that step six was wrong:

```
curl -X POST http://127.0.0.1:3000/scenario -d '[{"cpu": 50, "seconds": 5}, {"memory": 9000, "seconds": 5}]'
step 2: memory must be between 1 and 512
```

| Message                                          | Why                                                   |
|--------------------------------------------------|-------------------------------------------------------|
| `step 2: memory must be between 1 and 512`       | A value outside the limit of its endpoint             |
| `step 1: seconds is needed`                      | A step with an action and no duration                 |
| `step 1: unknown key 'crash'`                    | A key that is not an action, `seconds` or `workers`   |
| `step 1: more than one action (cpu and memory)`  | A step does one thing; put the other in the next step, or run them one after the other with a pause |
| `step 1: workers goes with cpu`                  | `workers` on another action                           |
| `step 1: nothing to do, name an action or give seconds for a pause` | An empty step                      |
| `The body must be a list of steps`               | The body is not a JSON list                           |

## Running, reading, stopping

| Request             | What it does                                                                  |
|---------------------|-------------------------------------------------------------------------------|
| `POST /scenario`    | Starts the scenario in the body: `202`, or `400` (see above), or `409` if one is running |
| `GET /scenario`     | Where it is: the state, the step, the seconds left and the plan with each step's state |
| `DELETE /scenario`  | Stops it: `200`, or `404` if none is running                                  |

Only one scenario runs at a time. A second `POST` while one is running is a `409` that says at which step it is; stop it first with `DELETE`, or wait for the end.

```
curl http://127.0.0.1:3000/scenario
```

```json
{
  "state": "running",
  "step": 2,
  "steps": 3,
  "secondsLeft": 18,
  "remainingSeconds": 38,
  "now": { "memory": 500, "seconds": 30 },
  "plan": [
    { "cpu": 80, "workers": 8, "seconds": 60, "state": "done" },
    { "memory": 500, "seconds": 30, "state": "running" },
    { "ready": "off", "seconds": 20, "state": "pending" }
  ]
}
```

The `state` of the scenario is `idle` (none has run), `running`, `finished`, `stopped` or `failed`, and the state of each step in the plan is `done`, `running`, `pending`, `stopped` or `failed`. `secondsLeft` is what remains of the current step and `remainingSeconds` of the whole scenario.

The last scenario stays readable after it ends, until the next one starts. `/info` has the same figures (`scenario`, without the plan) and the log says each step when it starts:

```
Scenario started: 3 steps
Scenario step 1 of 3: cpu 80% of 8 workers for 60 s
Scenario step 2 of 3: memory 500 MB for 30 s
```

### When a step can't run

The limits are checked at the start, but the budgets are shared with everyone else: if another request holds the memory when a step asks for it, the step can't have it. The scenario then stops there with the state `failed` and the reason in `error` (`step 2 (memory): 300 MB already held, limit is 512 MB`, the same words the endpoint answers with a `429`), and the steps after it don't run. Opening fewer descriptors or connections than asked, because the process hit its own limit, is not a failure: it is what `/fd` and `/connections` are for, and the step goes on (the log says how many were opened).

### Stopping

`DELETE /scenario` ends the sequence at once. The CPU load and the readiness switch of the current step are undone right away. What a step holds for a time (memory, disk, descriptors, connections) is given back when its seconds are over, as it would be after the same request on its own, so the budget is free again at most `seconds` later.

A `block` can't be stopped once it has started, and while it lasts the server doesn't answer, `GET /scenario` included. The scenario's own clock is frozen with the rest, so the step after the block starts when the server wakes up.

## Examples

**A ramp of CPU**, to watch an autoscaler add replicas and then remove them:

```
[
  {"cpu": 30, "seconds": 120},
  {"cpu": 60, "seconds": 120},
  {"cpu": 90, "seconds": 300},
  {"cpu": 0,  "seconds": 300}
]
```

The last step, with `cpu` at `0`, is the cool-down: it keeps the load off for five minutes, which is the time a scale-down is judged on.

**An instance that leaves the Service and comes back**, to test a readiness probe and what a load balancer does with it:

```
[
  {"seconds": 30},
  {"ready": "off", "seconds": 60},
  {"seconds": 30}
]
```

**Every budget in turn**, to see what each limit does to the instance one resource at a time, with a pause so the previous one is given back:

```
[
  {"memory": 400, "seconds": 60},
  {"disk": 500, "seconds": 60},
  {"fd": 5000, "seconds": 30},
  {"connections": 200, "seconds": 30}
]
```

Each step starts when the previous one ends, so the resources are not held together. To hold two at the same time, run the second with its own endpoint while the scenario goes: the budgets are shared, so that works.

**A hang in the middle of a test**, to see a liveness probe time out:

```
[
  {"cpu": 50, "seconds": 20},
  {"block": 15000},
  {"cpu": 50, "seconds": 20}
]
```

## From the command line

```
torchio scenario ramp.json --wait
```

starts the scenario in the file and stays until it ends, with the exit status `0` only if it finished: the line for a pipeline that loads an instance and then checks it. `torchio scenario status` and `torchio scenario stop` do the other two requests. See [Starting a test](starting-a-test.md#commands-that-send-or-read-a-body).

## Seeing it

- [`torchio watch`](watch.md) has a line for the scenario (`step 2 of 3, 18 s left in it, 38 s in all`, or why it failed) next to the other activity.
- The [control panel](control-panel.md) has a Scenario card: a box for the JSON, with an example, and the buttons to start and stop it. The step and the time left are in its header.
- `GET /scenario` and [`/info`](endpoints.md#info) for scripts.

## The token

All three requests need [`TOKEN`](token.md#token) when it is set, `GET` included, since it shows the whole plan. [`/info`](endpoints.md#info) stays open and shows where the scenario is.
