FROM node:22-bookworm-slim
WORKDIR /app

ENV PNPM_HOME=/pnpm
ENV PATH=$PNPM_HOME:$PATH
RUN corepack enable

COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile

ARG PUBLIC_API_BASE_URL=http://localhost:3001
ENV PUBLIC_API_BASE_URL=$PUBLIC_API_BASE_URL

COPY . .
RUN pnpm build

ENV HOST=0.0.0.0
ENV PORT=3000
EXPOSE 3000

CMD ["pnpm", "preview", "--host", "0.0.0.0", "--port", "3000"]
