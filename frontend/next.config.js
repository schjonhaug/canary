module.exports = {
  // All API calls handled by Next.js API routes with proper caching configuration
  serverExternalPackages: [],
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
  // Configure server options for API routes
  serverRuntimeConfig: {
    // Standard timeout for API routes
    apiTimeout: 30000, // 30 seconds
  },
  // Configure production settings
  poweredByHeader: false,
}; 