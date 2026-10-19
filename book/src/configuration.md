# Configuration

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
| `SHUTDOWN_GRACE_SECONDS` | `0`      | Seconds SIGTERM drains for before the process ends, see [Shutdown](shutdown.md#shutdown) |
| `MAX_FD`           | `10000`         | Total descriptors `/fd` can hold at once          |
| `MAX_CONNECTIONS`  | `500`           | Total connections `/connections` can hold at once |
| `MAX_BLOCK_MS`     | `30000`         | Highest `ms` accepted by `/block`                 |
| `ALLOW_OOM`        | off             | `1` lets `/oom` run: it allocates until the kernel kills the process |
| `MAX_LOG_LINES`    | `100000`        | Highest `lines` accepted by `/log`                |
| `MAX_LOG_RATE`     | `10000`         | Highest `rate` accepted by `/log`                 |
| `TRUST_PROXY`      | off             | Trusts `X-Forwarded-*` (`true`, a number of hops, or addresses and subnets), see `/echo` |
| `TOKEN`            | none            | Makes the endpoints that change the state need `Authorization: Bearer <token>` |
| `MAX_HOLD_SECONDS` | `300`           | Highest `hold`, `seconds` and `after` accepted by the endpoints that take them |
| `DISK_DIR`         | system temp dir | Directory where `/disk` writes its files          |
| `LOG_FORMAT`       | `text`          | `json` writes one JSON object a line instead of plain text, see [Logs](logs.md#logs) |

A number must be a positive integer. A value that is not one is reported at startup and replaced by the default.

```
docker run -p 3000:3000 -e WORKERS=2 -e MAX_MEMORY_MB=2048 altrdev/torchio:latest
```
