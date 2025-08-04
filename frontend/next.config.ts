import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Suppress specific server errors
  onDemandEntries: {
    // Period (in ms) where the server will keep pages in the buffer
    maxInactiveAge: 25 * 1000,
    // Number of pages that should be kept simultaneously without being disposed
    pagesBufferLength: 5,
  },
  // Disable strict mode to avoid double renders in development
  reactStrictMode: false,
};

export default nextConfig;
