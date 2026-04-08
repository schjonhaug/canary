import '@testing-library/jest-dom'

// Set default environment variables for tests
// Tests can override these as needed
process.env.NEXT_PUBLIC_CANARY_MODE = 'cloud'
process.env.NEXT_PUBLIC_API_URL = 'http://localhost:3000'

// Mock matchMedia for useIsMobile hook (only in jsdom environments)
if (typeof window !== 'undefined') {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: jest.fn().mockImplementation(query => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: jest.fn(),
      removeListener: jest.fn(),
      addEventListener: jest.fn(),
      removeEventListener: jest.fn(),
      dispatchEvent: jest.fn(),
    })),
  })
}

// Mock ResizeObserver
global.ResizeObserver = jest.fn().mockImplementation(() => ({
  observe: jest.fn(),
  unobserve: jest.fn(),
  disconnect: jest.fn(),
}))

// Mock next-intl with actual translations
jest.mock('next-intl', () => {
  const React = require('react')
  // Load translations inside the mock factory
  const translations = require('./messages/en-US.json')

  // Helper to get nested value from object using dot notation
  function getNestedValue(obj, path) {
    return path.split('.').reduce((current, key) => {
      return current && current[key] !== undefined ? current[key] : undefined
    }, obj)
  }

  function renderRichText(value, params = {}) {
    const plainParams = Object.fromEntries(
      Object.entries(params).filter(([, v]) => typeof v !== 'function')
    )

    let interpolated = value
    Object.entries(plainParams).forEach(([k, v]) => {
      interpolated = interpolated.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v))
    })

    const tagRegex = /<(\w+)>(.*?)<\/\1>/g
    const parts = []
    let lastIndex = 0
    let match

    while ((match = tagRegex.exec(interpolated)) !== null) {
      const [fullMatch, tagName, content] = match
      if (match.index > lastIndex) {
        parts.push(interpolated.slice(lastIndex, match.index))
      }

      const formatter = params[tagName]
      parts.push(typeof formatter === 'function' ? formatter(content) : content)
      lastIndex = match.index + fullMatch.length
    }

    if (lastIndex < interpolated.length) {
      parts.push(interpolated.slice(lastIndex))
    }

    if (parts.length === 0) return interpolated
    if (parts.length === 1) return parts[0]

    return React.createElement(
      React.Fragment,
      null,
      ...parts.map((part, index) => (
        React.isValidElement(part) ? React.cloneElement(part, { key: index }) : part
      ))
    )
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
        if (params && typeof value === 'string') {
          return renderRichText(value, params)
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
