import { applyTheme, getThemeInitializationScript, isThemePreference, resolveTheme } from "../theme"

describe("theme helpers", () => {
  it("recognizes valid theme preferences", () => {
    expect(isThemePreference("system")).toBe(true)
    expect(isThemePreference("light")).toBe(true)
    expect(isThemePreference("dark")).toBe(true)
    expect(isThemePreference("sepia")).toBe(false)
    expect(isThemePreference(null)).toBe(false)
  })

  it("resolves the system theme using the media query result", () => {
    expect(resolveTheme("system", true)).toBe("dark")
    expect(resolveTheme("system", false)).toBe("light")
    expect(resolveTheme("dark", false)).toBe("dark")
  })

  it("applies the dark class and color scheme to the document root", () => {
    applyTheme("dark")
    expect(document.documentElement.classList.contains("dark")).toBe(true)
    expect(document.documentElement.style.colorScheme).toBe("dark")

    applyTheme("light")
    expect(document.documentElement.classList.contains("dark")).toBe(false)
    expect(document.documentElement.style.colorScheme).toBe("light")
  })

  it("guards the initialization script when matchMedia is unavailable", () => {
    expect(getThemeInitializationScript()).toContain("typeof window.matchMedia === 'function'")
  })
})
