# syntax=docker/dockerfile:1

# The binary is compiled for the platform of the image on the platform that builds it, with
# cargo-zigbuild (zig is the linker), so a multi-platform build needs no emulation.
FROM --platform=$BUILDPLATFORM rust:1.99-slim-trixie AS build
ARG TARGETARCH
ARG CARGO_ZIGBUILD_VERSION=0.23.4
ARG ZIGLANG_VERSION=0.17.0
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3-venv \
    && rm -rf /var/lib/apt/lists/* \
    && python3 -m venv /opt/zig \
    && /opt/zig/bin/pip install --no-cache-dir cargo-zigbuild==${CARGO_ZIGBUILD_VERSION} ziglang==${ZIGLANG_VERSION}
ENV PATH=/opt/zig/bin:${PATH}

RUN case "${TARGETARCH}" in \
      amd64) echo x86_64-unknown-linux-musl ;; \
      arm64) echo aarch64-unknown-linux-musl ;; \
      *) echo "unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac > /target \
    && rustup target add "$(cat /target)"

WORKDIR /src
# the dependencies first, so they stay cached while the sources change
COPY Cargo.toml Cargo.lock ./
RUN mkdir src \
    && echo 'fn main() {}' > src/main.rs \
    && cargo zigbuild --release --locked --target "$(cat /target)" \
    && rm -rf src
COPY src ./src
COPY assets ./assets
# /tmp is where /disk writes by default, and the image has nothing else
RUN touch src/main.rs \
    && cargo zigbuild --release --locked --target "$(cat /target)" \
    && mkdir -p /rootfs/usr/local/bin /rootfs/tmp \
    && chmod 1777 /rootfs/tmp \
    && cp "target/$(cat /target)/release/torchio" /rootfs/usr/local/bin/torchio

# The binary alone, for each platform:
#   docker buildx build --target binary --platform linux/amd64,linux/arm64 --output type=local,dest=dist .
FROM scratch AS binary
COPY --from=build /rootfs/usr/local/bin/torchio /torchio

FROM scratch
COPY --from=build /rootfs/ /
ENV PATH=/usr/local/bin
USER 65532:65532
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
  CMD ["/usr/local/bin/torchio", "healthcheck"]
ENTRYPOINT ["/usr/local/bin/torchio"]
