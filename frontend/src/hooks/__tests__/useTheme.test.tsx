import { act, renderHook, waitFor } from "@testing-library/react"
import { THEME_STORAGE_KEY } from "@/lib/theme"
import { ThemeProvider, useTheme } from "../useTheme"

type MediaListener = (event: MediaQueryListEvent) => void

describe("useTheme", () => {
  let systemPrefersDark = false
  let mediaListeners: Set<MediaListener>

  function mockMatchMedia(options?: { legacyOnly?: boolean }) {
    const legacyOnly = options?.legacyOnly ?? false

    Object.defineProperty(window, "matchMedia", {
      writable: true,
      value: jest.fn().mockImplementation((query: string) => ({
        matches: systemPrefersDark,
        media: query,
        onchange: null,
        addEventListener: legacyOnly
          ? undefined
          : (_event: string, listener: MediaListener) => {
              mediaListeners.add(listener)
            },
        removeEventListener: legacyOnly
          ? undefined
          : (_event: string, listener: MediaListener) => {
              mediaListeners.delete(listener)
            },
        addListener: (listener: MediaListener) => {
          mediaListeners.add(listener)
        },
        removeListener: (listener: MediaListener) => {
          mediaListeners.delete(listener)
        },
        dispatchEvent: jest.fn(),
      })),
    })
  }

  beforeEach(() => {
    systemPrefersDark = false
    mediaListeners = new Set()
    window.localStorage.clear()
    mockMatchMedia()
  })

  afterEach(() => {
    jest.restoreAllMocks()
  })

  function wrapper({ children }: { children: React.ReactNode }) {
    return <ThemeProvider>{children}</ThemeProvider>
  }

  function emitSystemThemeChange(matches: boolean) {
    systemPrefersDark = matches

    act(() => {
      for (const listener of mediaListeners) {
        listener({ matches } as MediaQueryListEvent)
      }
    })
  }

  it("keeps an explicit theme when storage is unavailable and the system theme changes", async () => {
    jest.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("storage unavailable")
    })
    jest.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("storage unavailable")
    })

    const { result } = renderHook(() => useTheme(), { wrapper })

    act(() => {
      result.current.setPreference("dark")
    })

    expect(result.current.preference).toBe("dark")
    expect(result.current.resolvedTheme).toBe("dark")

    emitSystemThemeChange(false)

    await waitFor(() => {
      expect(result.current.preference).toBe("dark")
      expect(result.current.resolvedTheme).toBe("dark")
    })
  })

  it("updates the theme when another tab changes the stored preference", async () => {
    const { result } = renderHook(() => useTheme(), { wrapper })

    act(() => {
      window.dispatchEvent(
        new StorageEvent("storage", {
          key: THEME_STORAGE_KEY,
          newValue: "dark",
        })
      )
    })

    await waitFor(() => {
      expect(result.current.preference).toBe("dark")
      expect(result.current.resolvedTheme).toBe("dark")
    })
  })

  it("falls back to the legacy MediaQueryList listener API", async () => {
    mockMatchMedia({ legacyOnly: true })

    const { result } = renderHook(() => useTheme(), { wrapper })

    emitSystemThemeChange(true)

    await waitFor(() => {
      expect(result.current.resolvedTheme).toBe("dark")
    })
  })
})
