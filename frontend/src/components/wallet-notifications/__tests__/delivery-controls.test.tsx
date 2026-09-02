import { render } from "@testing-library/react"

import { PROVIDERS, ProviderIcon } from "../delivery-controls"

describe("ProviderIcon", () => {
  it.each(["ntfy", "nostr"] as const)("inverts the %s monochrome icon only in dark mode", (providerName) => {
    const provider = PROVIDERS.find(({ value }) => value === providerName)
    expect(provider).toBeDefined()

    const { container } = render(<ProviderIcon provider={provider!} />)
    const icon = container.querySelector("img")

    expect(icon).toHaveClass("dark:invert")
    expect(icon).not.toHaveClass("invert")
  })
})
