import type { NextConfig } from "next";

// Get build commit at config load time (works with both webpack and Turbopack)
const { getBuildCommit } = require('./scripts/generate-build-info');
const buildCommit = getBuildCommit();

const nextConfig: NextConfig = {
  // Enable Turbopack (Next.js 16 default)
  turbopack: {},
  // Enable standalone output for Docker deployment
  output: 'standalone',
  // Disable powered by header
  poweredByHeader: false,
  // Suppress specific server errors
  onDemandEntries: {
    // Period (in ms) where the server will keep pages in the buffer
    maxInactiveAge: 25 * 1000,
    // Number of pages that should be kept simultaneously without being disposed
    pagesBufferLength: 5,
  },
  // Disable strict mode to avoid double renders in development
  reactStrictMode: false,
  // Set build commit as environment variable
  env: {
    NEXT_PUBLIC_BUILD_COMMIT: buildCommit || '',
  },
  // Configure API routes with appropriate caching headers
  async headers() {
    return [
      {
        source: '/api/:path*',
        headers: [
          {
            key: 'Cache-Control',
            value: 'no-cache, no-transform',
          },
        ],
      },
    ];
  },
};

export default nextConfig;
