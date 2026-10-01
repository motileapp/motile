# The auth server (apps/auth). The host and the apps are built by the release workflow.

# ---------- rust toolchain ----------
# Pinned so a new Rust release doesn't throw away the cached dependency layers.
FROM rust:1.98-bookworm AS chef
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
    && cargo binstall -y cargo-chef@0.1.78
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---------- motile-auth ----------
# Only motile-auth and its dependencies are compiled; the other workspace members are just resolved.
FROM chef AS auth
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release -p motile-auth --recipe-path recipe.json
COPY . .
RUN cargo build --release -p motile-auth \
    && mkdir /out && mv target/release/motile-auth /out/ \
    && rm -rf target

# ---------- runtime ----------
FROM debian:bookworm-slim
LABEL org.opencontainers.image.source=https://github.com/motileapp/motile
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=auth /out/motile-auth /usr/local/bin/motile-auth
ENV PORT=3000 RUST_LOG=info
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s CMD curl -fsS http://127.0.0.1:3000/healthz || exit 1
CMD ["motile-auth"]
