# Changelog

All notable changes to this project are documented here. The GitHub release of each version uses the matching section.

## 2.3.0 - 2026-10-18

### Added
- `/delay` endpoint: answers after `ms` milliseconds without blocking the server.
- `/status/:code` endpoint: answers with the requested HTTP status (200-599).
- `/memory` endpoint: allocates `mb` megabytes and holds them for `hold` seconds.
- `/disk` endpoint: writes `mb` megabytes to a file in `DISK_DIR` and deletes it after `hold` seconds.
- `MAX_DELAY_MS`, `MAX_MEMORY_MB`, `MAX_DISK_MB`, `MAX_HOLD_SECONDS` and `DISK_DIR` environment variables.

### Changed
- README restructured with a section per endpoint; the changelog moved to this file.
- CI runs the built Docker image and checks the endpoints and the healthcheck.
- A GitHub release is created for every tag.

## 2.2.1 - 2026-10-18

### Changed
- Project renamed to `stress-node`. The Docker image is published as `altrdev/stress-node`; `altrdev/high-cpu-node` is deprecated and this is its last release.

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
