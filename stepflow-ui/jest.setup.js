import '@testing-library/jest-dom'

// Mock fetch for tests
globalThis.fetch = jest.fn();

// Mock Next.js router
jest.mock('next/navigation', () => ({
  useRouter() {
    return {
      push: jest.fn(),
      replace: jest.fn(),
      prefetch: jest.fn(),
    }
  },
  useSearchParams() {
    return new URLSearchParams()
  },
  usePathname() {
    return ''
  },
}))

// Mock environment variables
process.env.STEPFLOW_SERVER_URL = 'http://localhost:7837/api/v1'
process.env.NODE_ENV = 'test'

// Setup test environment with absolute URLs
global.location = {
  origin: 'http://localhost:3000',
  href: 'http://localhost:3000',
}

// Increase timeout for API integration tests
jest.setTimeout(30000)