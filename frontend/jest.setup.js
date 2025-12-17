import '@testing-library/jest-dom'

// Set default environment variables for tests
// Tests can override these as needed
process.env.NEXT_PUBLIC_CANARY_MODE = 'cloud'
process.env.NEXT_PUBLIC_API_URL = 'http://localhost:3000'

// Mock ResizeObserver
global.ResizeObserver = jest.fn().mockImplementation(() => ({
  observe: jest.fn(),
  unobserve: jest.fn(),
  disconnect: jest.fn(),
}))