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

if (typeof window !== 'undefined') {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: jest.fn().mockImplementation((query) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: jest.fn(),
      removeEventListener: jest.fn(),
      addListener: jest.fn(),
      removeListener: jest.fn(),
      dispatchEvent: jest.fn(),
    })),
  })
}

// Mock next-intl with actual translations
jest.mock('next-intl', () => {
  // Load translations inside the mock factory
  const translations = require('./messages/en-US.json')

  // Helper to get nested value from object using dot notation
  function getNestedValue(obj, path) {
    return path.split('.').reduce((current, key) => {
      return current && current[key] !== undefined ? current[key] : undefined
    }, obj)
  }

  return {
    useTranslations: (namespace) => {
      const namespaceData = namespace ? getNestedValue(translations, namespace) : translations

      const t = (key, params) => {
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

      // Add rich method for rich text formatting (returns plain text for tests)
      t.rich = (key, params) => {
        let value = getNestedValue(namespaceData, key)
        if (value === undefined) {
          value = getNestedValue(translations, `${namespace}.${key}`)
        }
        if (value === undefined) {
          return namespace ? `${namespace}.${key}` : key
        }
        // Handle parameter substitution (simple version - just substitute values)
        if (params && typeof value === 'string') {
          Object.entries(params).forEach(([k, v]) => {
            if (typeof v === 'function') {
              // For rich text handlers, extract the content from XML-like tags
              const tagRegex = new RegExp(`<${k}>([^<]*)</${k}>`, 'g')
              value = value.replace(tagRegex, (match, content) => v(content))
            } else {
              value = value.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v))
            }
          })
        }
        return value
      }

      // Add raw method for getting arrays
      t.raw = (key) => {
        let value = getNestedValue(namespaceData, key)
        if (value === undefined) {
          value = getNestedValue(translations, `${namespace}.${key}`)
        }
        return value
      }

      return t
    },
    useLocale: () => 'en-US',
    useMessages: () => translations,
    useFormatter: () => ({
      number: (value) => String(value),
      dateTime: (value) => String(value),
      relativeTime: (value) => String(value),
    }),
  }
})
