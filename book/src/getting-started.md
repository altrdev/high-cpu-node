# Try it in 30 seconds

Start the server with **Docker**:

```
docker run --name torchio -p 3000:3000 altrdev/torchio
```

Or on your local machine, with **Homebrew** (macOS and Linux):

```
brew install altrdev/tap/torchio
torchio serve
```

`torchio` with no command does the same as `torchio serve`: it is what the Docker image runs. The server stays in the foreground, so Ctrl+C stops it; use a second terminal tab for `torchio watch`.

Other ways to get the `torchio` command. The **install script** downloads the binary of the latest release for your system (macOS or Linux, `amd64` or `arm64`), checks its checksum and installs it:

```
curl -fsSL https://raw.githubusercontent.com/altrdev/torchio/main/install.sh | sh
```

It installs in `/usr/local/bin` (with `sudo` if needed, or in `~/.local/bin` when there is none). `TORCHIO_VERSION=v4.0.0` picks a release and `TORCHIO_INSTALL_DIR` the folder, for example `curl -fsSL ... | TORCHIO_INSTALL_DIR=$HOME/bin sh`. Or take the binary by hand from the [**releases**](https://github.com/altrdev/torchio/releases), or **build it from the source** with **cargo** (Rust 1.88 or later):

```
cargo install --git https://github.com/altrdev/torchio
```

In a second terminal, watch what it does:

```
docker exec -it torchio torchio watch    # with Docker
torchio watch                            # on your local machine, press m to start a memory test
```

In a third one, put it under load, and see the bars move:

```
torchio memory 200 --hold 20
torchio disk 100 --hold 20
torchio stress 44
```

The same thing with `curl`: `curl "http://127.0.0.1:3000/memory?mb=200&hold=20"`. See [Starting a test](starting-a-test.md#starting-a-test).
