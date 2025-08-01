module.exports = {
  // All API calls now handled by Next.js API routes for proper SSE support
  serverExternalPackages: [],
  // Configure API routes to handle long-running connections
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