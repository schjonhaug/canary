import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { SaveButton } from "../save-button"

describe("SaveButton", () => {
  it("renders the idle Save label", () => {
    render(<SaveButton />)

    expect(screen.getByRole("button", { name: /^save$/i })).toBeEnabled()
  })

  it("shows Saving while a save is in progress", () => {
    render(<SaveButton saving />)

    expect(screen.getByRole("button", { name: /saving/i })).toBeDisabled()
  })

  it("shows an animated saved checkmark after a successful save", () => {
    render(<SaveButton saved />)

    const savedButton = screen.getByRole("button", { name: /^saved$/i })
    expect(savedButton).toBeDisabled()
    expect(savedButton.querySelector("svg")).toHaveClass("canary-save-check")
  })

  it("calls onClick when Save is available", async () => {
    const user = userEvent.setup()
    const onClick = jest.fn()

    render(<SaveButton onClick={onClick} />)
    await user.click(screen.getByRole("button", { name: /^save$/i }))

    expect(onClick).toHaveBeenCalled()
  })
})
