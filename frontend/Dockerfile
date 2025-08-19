# Build stage
FROM node:22-alpine AS builder

WORKDIR /app

# Accept build arguments
ARG NEXT_PUBLIC_CANARY_MODE

# Set environment variables for build
ENV NEXT_PUBLIC_CANARY_MODE=$NEXT_PUBLIC_CANARY_MODE

# Copy package files
COPY package*.json ./

# Install dependencies
RUN npm ci

# Copy source code
COPY . .

# Build the application
RUN npm run build

# Runtime stage
FROM node:22-alpine

WORKDIR /app

# Copy package files
COPY package*.json ./

# Install only production dependencies
RUN npm ci --only=production && npm cache clean --force

# Copy built application from builder stage
COPY --from=builder /app/.next ./.next
COPY --from=builder /app/public ./public

# Create non-root user
RUN addgroup -g 1001 -S nodejs
RUN adduser -S nextjs -u 1001

USER nextjs

# Expose port
EXPOSE 3000

# Start the application
CMD ["npm", "start"]