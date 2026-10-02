# The web app (apps/web). Build from the repository root: docker build -f apps/web/Dockerfile .

FROM node:24-bookworm-slim AS build
RUN npm install -g pnpm@11.24.0
WORKDIR /app
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/site/package.json apps/site/
COPY apps/web/package.json apps/web/
RUN pnpm install --frozen-lockfile --filter motile-web
COPY apps/web apps/web
RUN pnpm --filter motile-web build

FROM node:24-bookworm-slim
LABEL org.opencontainers.image.source=https://github.com/motileapp/motile
WORKDIR /app
COPY --from=build /app/apps/web/.output .output
ENV PORT=3000 NODE_ENV=production
EXPOSE 3000
USER node
CMD ["node", ".output/server/index.mjs"]
