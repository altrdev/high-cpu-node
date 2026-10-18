# Changelog

All notable changes to this project are documented here. The GitHub release of each version uses the matching section.

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
