# The proxy's image, ghcr.io/motileapp/motile-proxy, which the release workflow builds from the static
# binaries its linux job made: bin/amd64/motile-proxy and bin/arm64/motile-proxy in the context.
FROM debian:bookworm-slim
LABEL org.opencontainers.image.source=https://github.com/motileapp/motile
ARG TARGETARCH
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY bin/${TARGETARCH}/motile-proxy /usr/local/bin/motile-proxy
ENV DATA_DIR=/data RUST_LOG=info
VOLUME /data
CMD ["motile-proxy"]
