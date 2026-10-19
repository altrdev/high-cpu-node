# Changelog

All notable changes to this project are documented here. The GitHub release of each version uses the matching section.

## Unreleased

### Added
- The Docker image is tagged with the version without the `v` (`X.Y.Z`), with the minor (`X.Y`) and with the major (`X`), as well as `vX.Y.Z` and `latest`.

## 4.1.1 - 2026-10-19

### Changed
- The completions list `serve` and `watch` first, then the commands that start a test, then `healthcheck`, `completions` and `help`.

### Fixed
- The Homebrew formula written by the release made the binary executable too late for the completions to be generated, and `brew install` failed with `EACCES`. The formula published for 4.1.0 was corrected in the tap.

### Documentation
- The README says what to check when Tab does not complete in zsh after `brew install`.

## 4.1.0 - 2026-10-19

### Added
- `torchio serve` starts the server, the same as `torchio` with no command, so the command is in `help` and in the completions. The Docker image still runs `torchio`.
- `torchio completions <bash|zsh|fish>` prints the completion script of a shell: the commands, their options and the words of `ready`. The Homebrew formula installs them with the binary.
- Tab completes the name of a command on the command line of `torchio watch`.

## 4.0.0 - 2026-10-19

### Changed
- torchio is rewritten in Rust and ships as a single static binary. The endpoints, parameters, limits, answers, environment variables and metrics are the same.
- The Docker image is the binary and an empty `/tmp`, about 1.4 MB, with no shell. It runs as the user `65532`, and its `HEALTHCHECK` runs `torchio healthcheck`. A directory used as `DISK_DIR` must be writable by that user.
- The server needs a few MB of memory instead of about 50, and the memory of `/memory` and `/leak` goes back to the OS as soon as it is released.
- The workers of `/stress` and `/cpu` are threads, not processes. `/stress` is about 1.7 times faster on the same machine: the same `number` keeps a core busy for less time.
- `WORKERS` defaults to the number of CPUs the process can use, which counts the CPU limit of a container (`--cpus 2` gives 2 workers). It used to be the number of cores of the machine.
- `watch` is a command of the binary: `torchio watch`, or `docker exec -it <container> torchio watch`. It speaks plain HTTP only.
- The help text of `torchio_workers` says threads instead of processes.

### Added
- `torchio healthcheck` exits with `0` when the server answers on `PORT`, for an image that has no `wget` or `curl`.
- `torchio cpu 80`, `torchio memory 512 --hold 60`, `torchio ready off 30`…: a command for each endpoint that changes the state of a server, so a test starts without `curl`. It takes `--url` and `--token` (or `TOKEN`).
- `torchio watch` has a new look on a terminal: smooth bars with a gradient, a header, a readiness badge, lit-up activity and the keys as chips. The plain text of `--once`, pipes and `NO_COLOR` is unchanged.
- On a terminal, `torchio watch` takes keys that start the tests (`c` cpu, `m` memory, `x` crash…), and `:` for any command. `--read-only` turns them off, `--token` sets the token, and `--once` or a pipe never take keys.
- The binary alone for `linux/amd64`, `linux/arm64`, `macos/amd64` and `macos/arm64`, with its `SHA256SUMS`, is attached to each GitHub release.
- `install.sh` installs the binary of the latest release for the system (Linux or macOS, `amd64` or `arm64`) after checking its checksum: `curl -fsSL https://raw.githubusercontent.com/altrdev/torchio/main/install.sh | sh`.
- `brew install altrdev/tap/torchio`: the release updates the formula of the Homebrew tap.

## 3.5.0 - 2026-10-19

### Added
- `GET /log?lines=&rate=` writes lines to stdout at a given rate, then stops (`MAX_LOG_LINES`, `MAX_LOG_RATE`). It needs the token with `TOKEN`. `/info` has `log` while it writes, and `watch` and the control panel show it.
- `GET /echo` returns the request as the server sees it (headers, client address, protocol), with `Authorization`, `Proxy-Authorization` and `Cookie` masked. `TRUST_PROXY` makes it follow `X-Forwarded-*`.
- `GET /help` lists every endpoint with its parameters, defaults and limits, as JSON or as plain text for a browser. `GET /config` shows the effective configuration, without the token. Both are open, and `/help` is built from the same definitions the endpoints read their parameters from (`src/endpoints.js`).

## 3.4.0 - 2026-10-18

### Added
- `TOKEN`: when set, `/stress`, `/cpu`, `/memory`, `/leak`, `/disk`, `/fd`, `/ready/off`, `/ready/on` and `/crash` need `Authorization: Bearer <token>` and answer `401` without it. `/`, `/ready`, `/info`, `/metrics`, `/delay` and `/status/:code` stay open, so probes and scrapers keep working. `/info` reports `auth`, and the control panel asks for the token when it gets a `401`. Without `TOKEN` nothing changes.

## 3.3.1 - 2026-10-18

### Added
- The control panel has a favicon (the logo). When the server is not ready or can't be reached, the tab turns red and its title says so, so you can tell from another tab.

## 3.3.0 - 2026-10-18

### Added
- A control panel in the browser at `/ui`, turned on with `UI=1` (off by default, since it makes `/crash` a button). It shows what `watch` shows, with the history of the last 60 seconds, plus the requests by status, the readiness and a card with sliders and buttons for every test. It is one static page: no new dependency, no build.

## 3.2.0 - 2026-10-18

### Added
- `/cpu?percent=&hold=`: keeps every worker busy for a share of the time, to hold the CPU at a level for a while (for CPU based autoscalers). `percent=0` stops it. It shows in `/info` (`stress.burn`), in `/metrics` (`torchio_cpu_burn_percent`) and in `watch`.
- `/leak?rate=&mb=&hold=`: memory that grows by itself, `rate` MB each second up to `mb`, then is held and released. It uses the budget of `/memory`.
- `START_DELAY_SECONDS`: the port stays closed for that many seconds after the start, to test startup probes and slow rollouts (default `0`).

## 3.1.1 - 2026-10-18

### Added
- `watch` shows the file descriptors held by `/fd` (a bar and its history) and the state of `/ready`, with the seconds left when it is not ready. Against a server older than 3.1 those two lines are left out.

### Changed
- The animation of the terminal view in the README shows the two new lines.

## 3.1.0 - 2026-10-18

### Added
- `/ready`, `/ready/off?seconds=` and `/ready/on`: a readiness check you can switch to `503` for a while or until you switch it back, while `/` keeps answering `200`. For readiness probes and load balancers.
- `/crash?code=&after=`: ends the process with the exit code you choose, to test restarts and backoff.
- `/fd?count=&hold=`: opens file descriptors and holds them, to test what happens when a process runs out of them (`EMFILE`). `MAX_FD` is the total that can be held at once (default `10000`).
- `/info` has `fd` and `ready`, `/metrics` has `torchio_fd_held`, `torchio_fd_limit` and `torchio_ready`.

### Changed
- The Kubernetes example uses `/ready` for the readiness probe.

## 3.0.1 - 2026-10-18

### Changed
- Documentation: the history of the names in the README and in this changelog was simplified.

## 3.0.0 - 2026-10-18

### Changed
- **Breaking:** the project is now called **Torchio** (it was `high-cpu-node` before). The Docker image is `altrdev/torchio`, the package is `torchio` and the repository is `github.com/altrdev/torchio`. `altrdev/high-cpu-node` stays deprecated.
- **Breaking:** the Prometheus metrics start with `torchio_`. `process_resident_memory_bytes` keeps its name.
- The `name` in `/info` is `torchio`, and the files of `/disk` are called `torchio-disk-*`.

## 2.5.3 - 2026-10-18

### Changed
- README reorganised to start with what the tool does: a short pitch, the animated terminal view, a 30 seconds try-out and scenarios (OOM kills, autoscaling, probes and timeouts, disk, metrics), followed by the reference.
- The README no longer shows the logo at the top, since the animation under the pitch does the same job (`docs/logo.svg` and `docs/logo.png` stay in the repo).
- `docs/watch.svg` is animated: the bars grow and shrink and the history scrolls while a load runs.

## 2.5.2 - 2026-10-18

### Added
- Tests with `node:test` (`npm test`): endpoints and limits, budgets, a crashing worker, an unusable `DISK_DIR`, the cleanup of the files, the configuration, `/metrics` and `watch`.

### Changed
- The code is split into modules under `src/` (one file for each endpoint, plus config, parameters, budget, worker pool and metrics). `server.js` only starts it, so `npm start` and the Docker image are unchanged. The answers of every endpoint, `/info` and `/metrics` are the same.
- A variable such as `WORKERS` or `MAX_NUMBER` that is not a positive integer is reported at startup and replaced by the default, instead of being ignored silently.
- Shutdown no longer detaches the exit handler of the workers: the pool knows it is stopping.
- CI runs the tests on Node 22, 24 and 26, fails when `CHANGELOG.md` has no section for the version in `package.json`, and validates `/metrics` with `promtool`.

### Fixed
- If `/memory` can't allocate the buffer, the memory it reserved is given back to the budget.

## 2.5.1 - 2026-10-18

### Changed
- The README copied to Docker Hub no longer has the badge line, which Docker Hub showed one badge per row.

## 2.5.0 - 2026-10-18

### Added
- `/metrics` endpoint: the figures of `/info` in the Prometheus text format, plus a counter of the requests answered, by route and status code.
- Banner for the repository social preview (`docs/social-preview.svg` and `docs/social-preview.png`).

### Changed
- The README badges are on one line, so Docker Hub shows them in a row.

## 2.4.1 - 2026-10-18

### Fixed
- `cli.js watch`: the disk directory is on its own line and shortened from the left, so a long path no longer wraps the free space onto another line.

## 2.4.0 - 2026-10-18

### Added
- `/info` endpoint: JSON with the requests in progress, the memory and disk held with their limits, the process memory, the container memory limit and the free disk space.
- `cli.js watch` (`npm run watch`): terminal view of `/info` with bars and a short history. It also works inside a container with `docker exec`.

## 2.3.2 - 2026-10-18

### Added
- Project logo (`docs/logo.svg` and `docs/logo.png`), shown in the README.
- README: status badges, Docker Compose and Kubernetes examples, security note.

### Changed
- `npm run dev` uses `node --watch` instead of `nodemon`, which is no longer a dependency.

## 2.3.1 - 2026-10-18

### Changed
- The changelog moved from the README to this file.
- A GitHub release is created for every tag, using the matching section of this file as notes.

## 2.3.0 - 2026-10-18

### Added
- `/delay` endpoint: answers after `ms` milliseconds without blocking the server.
- `/status/:code` endpoint: answers with the requested HTTP status (200-599).
- `/memory` endpoint: allocates `mb` megabytes and holds them for `hold` seconds.
- `/disk` endpoint: writes `mb` megabytes to a file in `DISK_DIR` and deletes it after `hold` seconds.
- `MAX_DELAY_MS`, `MAX_MEMORY_MB`, `MAX_DISK_MB`, `MAX_HOLD_SECONDS` and `DISK_DIR` environment variables.

### Changed
- README restructured with a section per endpoint.
- CI runs the built Docker image and checks the endpoints and the healthcheck.

## 2.2.1 - 2026-10-18

### Changed
- The image `altrdev/high-cpu-node` is deprecated, and this is its last release.

## 2.2.0 - 2026-10-18

### Changed
- Updated to Express 5, the `node:26-alpine` base image and `cors` 2.8.6.

## 2.1.0 - 2026-10-18

### Added
- `HEALTHCHECK` in the Docker image.
- The Docker Hub description is synced from the README on release.

## 2.0.0 - 2026-10-17

### Added
- Worker pool, one process per core, with automatic restart of crashed workers.
- `PORT`, `WORKERS` and `MAX_NUMBER` environment variables.

### Changed
- **Breaking:** `/stress` rejects a `number` outside `0..MAX_NUMBER` with a `400` (it was unbounded).
