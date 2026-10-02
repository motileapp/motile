# The marketing site (apps/marketing), as static files behind static-web-server. Build from the
# repository root: docker build -f apps/marketing/Dockerfile .

FROM node:24-bookworm-slim AS build
RUN npm install -g pnpm@11.24.0
WORKDIR /app
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/marketing/package.json apps/marketing/
COPY apps/web/package.json apps/web/
RUN pnpm install --frozen-lockfile --filter motile-marketing
COPY apps/marketing apps/marketing
RUN pnpm --filter motile-marketing build

FROM joseluisq/static-web-server:2.44.0
LABEL org.opencontainers.image.source=https://github.com/motileapp/motile
COPY --from=build /app/apps/marketing/dist /public
COPY apps/marketing/server.toml /server.toml
ENV SERVER_CONFIG_FILE=/server.toml
EXPOSE 3000
