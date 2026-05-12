# Build stage
FROM node:22-alpine AS builder

# Install pnpm
RUN corepack enable && corepack prepare pnpm@10 --activate

WORKDIR /app

# Accept build arguments
ARG NEXT_PUBLIC_CANARY_MODE
ARG NEXT_PUBLIC_API_URL
ARG NEXT_PUBLIC_BUILD_COMMIT

# Set environment variables for build
ENV NEXT_PUBLIC_CANARY_MODE=$NEXT_PUBLIC_CANARY_MODE
ENV NEXT_PUBLIC_API_URL=$NEXT_PUBLIC_API_URL
ENV NEXT_PUBLIC_BUILD_COMMIT=$NEXT_PUBLIC_BUILD_COMMIT

# Copy package files and pnpm build-script approvals
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./

# Install dependencies
RUN pnpm install --frozen-lockfile

# Copy source code
COPY . .

# Build the application with webpack (needed for standalone output and custom webpack config)
RUN npx next build --webpack

# Runtime stage
FROM node:22-alpine

WORKDIR /app

ENV NODE_ENV=production

# Create non-root user
RUN addgroup --system --gid 1001 nodejs && \
    adduser --system --uid 1001 nextjs

# Copy public assets
COPY --from=builder /app/public ./public

# Copy standalone build (includes bundled node_modules)
COPY --from=builder --chown=nextjs:nodejs /app/.next/standalone ./

# Copy static files
COPY --from=builder --chown=nextjs:nodejs /app/.next/static ./.next/static

USER nextjs

EXPOSE 3000

ENV PORT=3000
ENV HOSTNAME="0.0.0.0"

# Run with node directly (not npm start)
CMD ["node", "server.js"]
