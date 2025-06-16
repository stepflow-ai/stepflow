import '@testing-library/jest-dom'

globalThis.fetch = require('node-fetch');

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
process.env.STEPFLOW_API_URL = 'http://localhost:7837/api/v1'

// Increase timeout for API integration tests
jest.setTimeout(30000)