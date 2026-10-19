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
| `torchio block [ms]`             | [`/block`](endpoints.md#block)       |
| `torchio ready on \| off [seconds]` | [`/ready`](endpoints.md#ready)   |
| `torchio scenario <file\|-> [--wait]`, `status`, `stop` | [`/scenario`](scenarios.md) |
| `torchio payload get [mb]`, `post <file\|->` | [`/payload`](endpoints.md#payload) |

The values after the name are the parameters of the endpoint, in the order of the table; any of them can also be given by name, as `--hold 60` or `--hold=60`. The limits are the ones of the server: a value it refuses is printed with its `400` and the command exits with `1`. With [`TOKEN`](token.md#token), pass `--token <token>` or set `TOKEN`. `/stress`, `/delay` and `/flaky` with `ms` answer when they finish, so the command waits for them. Like `watch`, it speaks plain HTTP only.

## Commands that send or read a body

`scenario` and `payload` don't fit the table above: they take a file, or send and read a body, and not only numbers. They take `--url` and `--token` (or `TOKEN`) like the others.

### `torchio scenario`

```
torchio scenario ramp.json            # start the scenario in the file
torchio scenario ramp.json --wait     # and follow it until it ends
cat ramp.json | torchio scenario -    # from stdin
torchio scenario status               # where it is
torchio scenario stop                 # end it
```

It prints what the server answers, written out. A mistake in the file is the `400` of the server, which names the step (`step 2: memory must be between 1 and 512`), and the command exits with `1`. With `--wait` the command stays until the scenario ends and prints each step as it starts (on the error output); the exit status is `0` if the scenario **finished** and `1` if it failed, was stopped or the server could not be reached for good. That makes it the line to put in a pipeline: `torchio scenario ramp.json --wait && ./check-the-metrics.sh`. The words `status` and `stop` are reserved: for a file with that name write `./status`. The steps are explained in [Scenarios](scenarios.md).

### `torchio payload`

```
torchio payload get 100               # download 100 MB, throw them away
torchio payload post backup.tar       # upload a file
head -c 50000000 /dev/zero | torchio payload post -
```

`get` reads the body as it arrives, without keeping it, and prints how much came, how long it took and the speed (`Received 100.0 MB in 2.31 s (43.3 MB/s)`). If the server announced a length and the body ends before it, because a proxy or a timeout cut it, that is an error with `1`: it is the thing the command is for. `post` sends a file, or what is piped in, and prints what the server counted. A body over `MAX_PAYLOAD_MB` is a `413`, and a proxy with a smaller limit of its own may close the connection before answering, which the command says too (`The connection was closed while the body was being sent`). The limits and what each endpoint does are in [`/payload`](endpoints.md#payload).

