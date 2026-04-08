import { act, renderHook } from "@testing-library/react"
import { useContactWizard } from "../useContactWizard"

const baseProps = {
  name: "Alice",
  enabledProviders: { email: true },
  providerValues: { email: "alice@example.com" },
  ntfyTopic: "",
  smsVerificationRequired: false,
  emailVerificationRequired: false,
  smsVerified: false,
  emailVerified: false,
}

describe("useContactWizard", () => {
  it("keeps the verification step after requirements clear once the user has entered it", () => {
    const { result, rerender } = renderHook(props => useContactWizard(props), {
      initialProps: {
        ...baseProps,
        emailVerificationRequired: true,
      },
    })

    act(() => {
      result.current.goNext()
    })
    expect(result.current.currentStep).toBe(1)

    act(() => {
      result.current.goNext()
    })
    expect(result.current.currentStep).toBe(2)
    expect(result.current.totalSteps).toBe(3)

    rerender({
      ...baseProps,
      emailVerificationRequired: false,
      emailVerified: true,
    })

    expect(result.current.currentStep).toBe(2)
    expect(result.current.totalSteps).toBe(3)
    expect(result.current.isLastStep).toBe(true)
  })
})
