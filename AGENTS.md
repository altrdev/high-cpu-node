# AGENTS.md

Torchio is a small HTTP server in Rust (a single static binary) that burns CPU, holds memory, fills disk and more, on demand, to test how containers behave under pressure. This file is for coding agents; `README.md` is for users and the documentation lives in `book/`.

## Commands

```
cargo build
cargo test --locked                                  # unit, router and integration tests (these start the real binary)
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked
```

CI (`.github/workflows/ci.yml`) runs all of the above on Linux and macOS, so run them before pushing. `rustfmt.toml` sets a 140 column width.

## Layout

- `src/routes/`: one file per HTTP endpoint (`cpu.rs`, `memory.rs`, `disk.rs`, ...), registered in `routes/mod.rs`.
- `src/resources/`: the state behind the endpoints (what is held, budgets, limits).
- `src/cli/`: the subcommands (`serve`, `watch`, `scenario`, ...). They call the same endpoints.
- `src/http/`: auth, params, content negotiation, counters.
- `src/endpoints.rs`: the catalog of endpoints and commands. `/help` and the endpoint reference in the book are built from it, and a test checks that every command calls an endpoint of the catalog.
- `assets/ui.html`: the control panel, a single self-contained page.
- `tests/`: integration tests; shared helpers are in `tests/common`.
- `book/`: documentation (mdBook), published at <https://torchio.altr.dev>. `book/src/reference.md` is generated, do not edit it by hand: `cargo run --example endpoints_reference > book/src/reference.md`. `book/src/development.md` has the full module map.
- `skills/torchio/SKILL.md`: the skill for agents that *use* torchio (not for working on this repo). `book/llms.py` writes `llms.txt` and `llms-full.txt` from the book. Keep the skill in line with the endpoints when they change.
- `deploy/`: Kubernetes manifest and Helm chart.

## Conventions

- Keep the binary and the image small: release profile is `opt-level = "z"` with LTO, and CI fails if the image is over 15 MB. Think twice before adding a dependency.
- A new or changed endpoint needs: the route, an entry in the catalog, its CLI command, a card in the control panel when it makes sense, `/info` and `watch` if it holds something, the shell completions, the docs in `book/`, tests, and a `CHANGELOG.md` entry.
- Every change that a user can notice goes under `## Unreleased` in `CHANGELOG.md` (Added, Changed, Fixed, Documentation). CI fails if the current version has no section there.
- Commits and branches follow Conventional Commits: `feat:`, `fix:`, `docs:`, `test:`, `chore:`, on branches like `feat/...` or `fix/...`, merged to `main` by pull request.
- Never edit versions by hand. `scripts/release.sh X.Y.Z` writes them in `Cargo.toml`, `Cargo.lock`, `deploy/` and the changelog, and CI checks that they agree. The release is committed as `chore: release X.Y.Z`, merged, and then the tag `vX.Y.Z` is pushed by the maintainer.
- Documentation and changelog are written in plain English, in the project's own words. The project is Rust only: do not mention Node or JavaScript, nor the name `stress-node`. The story in the README starts at `high-cpu-node` (2019).
- Commit only when asked, one commit per step. Pushes, tags and Docker Hub are done by the maintainer.
- Images for the docs go in `docs/` only.

## Notes

- This server consumes resources on purpose and is not meant to be exposed to the public internet. `TOKEN` protects the endpoints that act.
- Integration tests start the real binary, so they need a built target; read the last lines of the server output after it has exited, not before.
