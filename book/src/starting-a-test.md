# Starting a test

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
| `torchio stress [number]`        | [`/stress`](endpoints.md#stress)     |
| `torchio cpu [percent] [hold] [workers]` | [`/cpu`](endpoints.md#cpu)   |
| `torchio delay [ms]`             | [`/delay`](endpoints.md#delay)       |
| `torchio status <code>`          | [`/status/:code`](endpoints.md#statuscode) |
| `torchio flaky [rate] [code] [ms]` | [`/flaky`](endpoints.md#flaky)     |
| `torchio memory [mb] [hold]`     | [`/memory`](endpoints.md#memory)     |
| `torchio leak [rate] [mb] [hold]`| [`/leak`](endpoints.md#leak)         |
| `torchio disk [mb] [hold]`       | [`/disk`](endpoints.md#disk)         |
| `torchio fd [count] [hold]`      | [`/fd`](endpoints.md#fd)             |
| `torchio connections [count] [hold]` | [`/connections`](endpoints.md#connections) |
| `torchio log [lines] [rate]`     | [`/log`](endpoints.md#log)           |
| `torchio crash [code] [after]`   | [`/crash`](endpoints.md#crash)       |
| `torchio ready on \| off [seconds]` | [`/ready`](endpoints.md#ready)   |

The values after the name are the parameters of the endpoint, in the order of the table; any of them can also be given by name, as `--hold 60` or `--hold=60`. The limits are the ones of the server: a value it refuses is printed with its `400` and the command exits with `1`. With [`TOKEN`](token.md#token), pass `--token <token>` or set `TOKEN`. `/stress`, `/delay` and `/flaky` with `ms` answer when they finish, so the command waits for them. Like `watch`, it speaks plain HTTP only.
