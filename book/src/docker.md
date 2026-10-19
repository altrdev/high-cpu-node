# Docker

The image is built for `linux/amd64` and `linux/arm64`. It holds the static binary and an empty `/tmp`, about 1.4 MB in all, and nothing else: no shell and no package manager, so `docker exec` runs `torchio` itself (`torchio watch`) rather than a shell. It runs as the non-root user `65532` and has a `HEALTHCHECK` on `/`, which runs `torchio healthcheck`. With `START_DELAY_SECONDS` the container is unhealthy until the port opens, so give the healthcheck a longer `--start-period`. Because of the non-root user, a directory you use as `DISK_DIR` (for example a mounted volume) must be writable by it.

Each release is tagged `X.Y.Z` (the exact version), `X.Y` and `X` (the latest patch of the minor and of the major), `vX.Y.Z` and `latest`. `latest`, `X.Y` and `X` move with each release, so pin the exact version where a change must not come by itself. The [tags on Docker Hub](https://hub.docker.com/r/altrdev/torchio/tags) of the first releases only have the `v`.

The binary alone, for Linux and macOS on `amd64` and `arm64`, is attached to each [release](https://github.com/altrdev/torchio/releases) with its `SHA256SUMS`.
