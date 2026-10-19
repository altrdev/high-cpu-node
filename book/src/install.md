# Install

Pick one way to get torchio. It is a single static binary for Linux and macOS (`amd64` and `arm64`), and an image of about 1.4 MB.

## Docker

```
docker pull altrdev/torchio
```

The image holds the binary and nothing else. The tags and the rest are in [Docker](docker.md).

## Homebrew

For macOS and Linux:

```
brew install altrdev/tap/torchio
```

It installs the shell completions too. If Tab does nothing in zsh, see [Shell completion](shell-completion.md).

## Install script

A script that downloads the binary of the latest release for your system (macOS or Linux, `amd64` or `arm64`), checks its checksum and installs it:

```
curl -fsSL https://raw.githubusercontent.com/altrdev/torchio/main/install.sh | sh
```

It installs in `/usr/local/bin` (with `sudo` if needed, or in `~/.local/bin` when there is none). `TORCHIO_VERSION=v4.0.0` picks a release and `TORCHIO_INSTALL_DIR` the folder, for example `curl -fsSL ... | TORCHIO_INSTALL_DIR=$HOME/bin sh`.

## Release binary

Take the binary of your system by hand from the [releases](https://github.com/altrdev/torchio/releases): `torchio-linux-amd64`, `torchio-linux-arm64`, `torchio-macos-amd64` or `torchio-macos-arm64`. `SHA256SUMS` is there to check it (`shasum -a 256 -c` on macOS):

```
curl -LO https://github.com/altrdev/torchio/releases/latest/download/torchio-linux-amd64
curl -LO https://github.com/altrdev/torchio/releases/latest/download/SHA256SUMS
grep ' torchio-linux-amd64$' SHA256SUMS | sha256sum -c
chmod +x torchio-linux-amd64
```

## From source

Build it with cargo (Rust 1.88 or later):

```
cargo install --git https://github.com/altrdev/torchio
```
