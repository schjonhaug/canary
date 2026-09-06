import { render } from "@testing-library/react"

import { PROVIDERS, ProviderIcon } from "../delivery-controls"

describe("ProviderIcon", () => {
  it.each(["ntfy", "nostr"] as const)("inverts the %s monochrome icon only in dark mode", (providerName) => {
    const provider = PROVIDERS.find(({ value }) => value === providerName)
    expect(provider).toBeDefined()

    const { container } = render(<ProviderIcon provider={provider!} />)
    const icon = container.querySelector("img")

    expect(icon).toBeInTheDocument()
    expect(icon).toHaveClass("dark:invert")
    expect(icon).not.toHaveClass("invert")
  })

  it("leaves full-color image providers uninverted", () => {
    const provider = {
      imageSrc: "/images/notifications/full-color.svg",
      invertInDarkMode: false,
    } as unknown as (typeof PROVIDERS)[number]

    const { container } = render(<ProviderIcon provider={provider} />)
    const icon = container.querySelector("img")

    expect(icon).toBeInTheDocument()
    expect(icon).not.toHaveClass("dark:invert")
    expect(icon).not.toHaveClass("invert")
  })
})
