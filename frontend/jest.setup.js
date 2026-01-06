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

// Mock next-intl with actual translations
jest.mock('next-intl', () => {
  // Load translations inside the mock factory
  const translations = require('./messages/en.json')

  // Helper to get nested value from object using dot notation
  function getNestedValue(obj, path) {
    return path.split('.').reduce((current, key) => {
      return current && current[key] !== undefined ? current[key] : undefined
    }, obj)
  }

  return {
    useTranslations: (namespace) => {
      const namespaceData = namespace ? getNestedValue(translations, namespace) : translations
      return (key, params) => {
        // Get the value from the namespace
        let value = getNestedValue(namespaceData, key)

        // Fall back to full path if not found in namespace
        if (value === undefined) {
          value = getNestedValue(translations, `${namespace}.${key}`)
        }

        // Return the key if translation not found
        if (value === undefined) {
          return namespace ? `${namespace}.${key}` : key
        }

        // Handle parameter substitution
        if (params && typeof value === 'string') {
          Object.entries(params).forEach(([k, v]) => {
            value = value.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v))
          })
        }

        return value
      }
    },
    useLocale: () => 'en',
    useMessages: () => translations,
    useFormatter: () => ({
      number: (value) => String(value),
      dateTime: (value) => String(value),
      relativeTime: (value) => String(value),
    }),
  }
})