# Development

```
cargo run               # starts the server
cargo run -- watch      # terminal view of a running server
cargo test              # every test: unit, router and integration
cargo clippy --all-targets -- -D warnings
cargo fmt
```

```
src/
  main.rs          the program: runs the server or one of the commands
  lib.rs           the crate, with a line on each module
  config.rs        environment variables, validated
  endpoints.rs     the parameters and limits of every endpoint, shared by the routes and /help
  app.rs           what the routes share, and the router
  actions.rs       what the endpoints that change the state do and say, shared by the routes and the scenarios
  server.rs        listens, and ends the process
  routes/          one file for each endpoint: reads the parameters, asks resources, writes the answer
  resources/       what the server holds or does on purpose, without knowing about HTTP
    budget.rs        the total that /memory, /disk and /fd share between requests
    memory.rs        memory taken from the OS (/memory, /leak)
    disk.rs          the files of /disk
    fd.rs            the descriptors of /fd
    connections.rs   the connections of /connections
    workers.rs       the pool of worker threads for /stress and /cpu
    ready.rs         the readiness switch
    shutdown.rs      the drain after SIGTERM, and what is open
    log.rs           the lines of /log
    delay.rs         the requests of /delay still waiting
    flaky.rs         the coin of /flaky: which requests fail
  http/            what serves requests and is not specific to an endpoint
    params.rs        the integer query parameters
    auth.rs          TOKEN
    trust.rs         TRUST_PROXY
    accept.rs        JSON or text for /help
    counters.rs      requests answered, for /metrics
    open.rs          the listener that counts connections and stops accepting, and the count of requests
  cli/             the commands besides the server
    watch.rs         torchio watch
    view.rs          what watch draws
    info.rs          what /info says, as watch reads it
    http.rs          the small HTTP client of the commands
    healthcheck.rs   torchio healthcheck
  logging.rs       LOG_FORMAT: the text and the JSON lines, and the access log
  snapshot.rs      the figures behind /info and /metrics
  system.rs        RSS, the memory limit of the container, free space, names of errors
  units.rs         units of size
assets/ui.html     the control panel, a single static page
tests/             integration tests, by area
deploy/            Kubernetes manifests and the Helm chart
book/              this documentation, with mdBook (`SUMMARY.md` is the list of pages)
examples/          the reference of the endpoints, written from the catalog
Dockerfile         builds the binary for both platforms with cargo-zigbuild, into an image of one file
```

The tests come in three kinds, and `cargo test` runs them all:

- **Unit tests** live in the module they test (`#[cfg(test)]`): the budgets, the parsing of parameters and `TRUST_PROXY`, the view of `watch`, the worker pool, the timers (with a paused clock).
- **Router tests** (`tests/api.rs`) call the router without a socket: every endpoint of the catalog is routed and protected as it says, every parameter is rejected one step outside its range, CORS, methods, content negotiation, and the shape of `/info`, `/config` and `/metrics`.
- **Integration tests** (`tests/*.rs`, with what they share in `tests/common`) start the real binary on a free port with small limits and call it over HTTP: `probes` (`/`, `/status`, `/ready`, `/delay`), `resources` (`/memory`, `/leak`, `/disk`, `/fd`), `load` (`/stress`, `/cpu`), `payload` (`/payload`), `scenario` (`/scenario`), `observability` (`/info`, `/metrics`, `/log`, `/echo`, `/help`, `/config`), `lifecycle` (configuration, start, signals, `/crash`), `auth` (`TOKEN`), `ui` and `cli` (`watch`, `healthcheck`).

CI runs them on Linux and macOS with the format check, `clippy` and the documentation, builds the image for both platforms, runs it with a memory limit and a CPU limit, and validates `/metrics` with `promtool`.

## The documentation site

The pages are in `book/src`, written in Markdown, and `book/book.toml` and `book/theme` set the title and the colours. The images are in `docs/`, where the README takes them from, and the second command below copies them next to the pages. The reference of the endpoints is not written by hand: `cargo run --example endpoints_reference > book/src/reference.md` writes it from the catalog that `/help` is built from, so it is always right. With [mdBook](https://rust-lang.github.io/mdBook/) installed:

```
cargo run --example endpoints_reference > book/src/reference.md
mkdir -p book/src/images && cp docs/* book/src/images/
mdbook serve book --open
python3 book/check-links.py book/book     # after mdbook build book
```

The version, in a small label next to the title and on the first page, is written by the workflow from `Cargo.toml`; on your machine they say it is the development version. The `Documentation` workflow builds the site for a pull request, to catch a mistake before the merge. It publishes to GitHub Pages when a release is tagged, so the site describes the version that can be installed, and when it is run by hand (*Actions*, *Documentation*, *Run workflow*) to publish a correction of the pages in between.

## Releasing

`scripts/release.sh X.Y.Z` writes the version in `Cargo.toml`, `Cargo.lock`, the image tag of `deploy/kubernetes/deployment.yaml` and the `version` and `appVersion` of the Helm chart, and dates the `Unreleased` section of the changelog. It changes nothing else: review the diff, commit it as `chore: release X.Y.Z`, merge it and push the tag `vX.Y.Z`, which publishes the image, the binaries and the documentation. CI fails when one of these places is left behind.
