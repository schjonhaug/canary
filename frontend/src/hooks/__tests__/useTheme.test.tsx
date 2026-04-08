import { act, renderHook, waitFor } from "@testing-library/react"
import { THEME_STORAGE_KEY } from "@/lib/theme"
import { useTheme } from "../useTheme"

type MediaListener = (event: MediaQueryListEvent) => void

describe("useTheme", () => {
  let systemPrefersDark = false
  let mediaListeners: Set<MediaListener>

  beforeEach(() => {
    systemPrefersDark = false
    mediaListeners = new Set()
    window.localStorage.clear()

    Object.defineProperty(window, "matchMedia", {
      writable: true,
      value: jest.fn().mockImplementation((query: string) => ({
        matches: systemPrefersDark,
        media: query,
        onchange: null,
        addEventListener: (_event: string, listener: MediaListener) => {
          mediaListeners.add(listener)
        },
        removeEventListener: (_event: string, listener: MediaListener) => {
          mediaListeners.delete(listener)
        },
        addListener: jest.fn(),
        removeListener: jest.fn(),
        dispatchEvent: jest.fn(),
      })),
    })
  })

  afterEach(() => {
    jest.restoreAllMocks()
  })

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

    const { result } = renderHook(() => useTheme())

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
    const { result } = renderHook(() => useTheme())

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
})
