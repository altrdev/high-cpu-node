# Watching a server

`torchio watch` shows what a server is doing in the terminal, refreshing every second with bars and a short history of the last samples (workers, memory, process memory, disk and file descriptors), plus whether `/ready` is answering `200` or `503`, a line for each `/cpu`, `/memory`, `/disk`, `/fd` and `/connections` that is running with the seconds it has left (`/memory: 100 MB, 18 s left`), and a line for the [scenario](scenarios.md) while one runs or after it ends. It is a command of the same binary, reads [`/info`](endpoints.md#info) and works against any running instance:

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

## Keys

On a terminal, `watch` also starts the tests, so you can load a server and see the bars move in the same window. A key opens a command line with the command already written, you add the values and press Enter:

| Key | Command | Key | Command |
|-----|---------|-----|---------|
| `c` | `cpu`   | `g` | `log`   |
| `m` | `memory`| `s` | `stress`|
| `l` | `leak`  | `r` | `ready` |
| `d` | `disk`  | `x` | `crash` |
| `f` | `fd`    | `:` | any command, like `status 503` |

The line shows the parameters of the command while you type (`cpu [percent] [hold]`), Enter with only the name uses the defaults, Esc cancels and `q` or Ctrl+C quits. The answer of the server, or its error, stays under the keys. The request runs in the background, so the view keeps refreshing while `/stress` or `/delay` wait. The commands are the ones of [Starting a test](starting-a-test.md#starting-a-test).

The keys are on only when both the input and the output are a terminal. `--read-only` turns them off, which is the way to watch a shared instance without a key that can [`/crash`](endpoints.md#crash) it. With `--once`, or with the output in a pipe or a file, `watch` never takes keys.

It speaks plain HTTP: to watch a server behind HTTPS, reach it with a port-forward. `watch` needs a server of version 4.5.0 or newer, and says so with the version of the server when it is older. A newer server is fine. The history only covers the time the command has been running. On a terminal the bars are smooth and shift from green to red as they get close to their limit (blue where there is no limit to compare with), the history is brighter when it is higher, and what is happening (`/stress`, `/cpu`, `/log`) lights up while the rest stays dim. Readiness is a badge, green or red. Set `NO_COLOR=1` for plain text, which is also what you get when the output is not a terminal.
