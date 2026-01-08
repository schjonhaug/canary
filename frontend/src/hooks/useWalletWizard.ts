import { useMemo, useEffect, useCallback } from "react"
import { useRouter } from "next/navigation"
import { walletGuides, type WalletGuide } from "@/lib/wallet-guides"
import { SAMPLE_WALLET_SLUG, SAMPLE_WALLETS } from "@/components/add-wallet-form"
import type { WizardStep } from "@/components/wallet-wizard/wizard-breadcrumb"

interface UseWalletWizardOptions {
  slug?: string[]
  network: string
}

interface UseWalletWizardReturn {
  step: WizardStep
  selectedWallet: WalletGuide | null
  isBaconWallet: boolean
  baconWallet: { name: string; descriptor: string }
  handleNavigateToChoose: () => void
  handleSelectWallet: (wallet: WalletGuide) => void
  handleSkipToForm: () => void
  handleSelectSampleWallet: () => void
  getGuideSteps: (walletId: string, t: { raw: (key: string) => unknown }) => string[]
}

export function useWalletWizard({ slug, network }: UseWalletWizardOptions): UseWalletWizardReturn {
  const router = useRouter()

  // Get bacon wallet for current network
  const baconWallet = SAMPLE_WALLETS[network as keyof typeof SAMPLE_WALLETS] || SAMPLE_WALLETS.mainnet

  // Check if we're using the Bacon sample wallet
  const isBaconWallet = slug?.[0] === SAMPLE_WALLET_SLUG

  // Derive selected wallet from URL
  const selectedWallet = useMemo(() => {
    if (!slug || slug.length === 0) return null
    if (slug[0] === 'form' || slug[0] === SAMPLE_WALLET_SLUG) return null
    return walletGuides.find(w => w.id === slug[0]) || null
  }, [slug])

  // Derive current step from URL
  const step: WizardStep = useMemo(() => {
    if (!slug || slug.length === 0) return 'choose'
    if (slug[0] === 'form' || slug[0] === SAMPLE_WALLET_SLUG) return 'form'
    if (selectedWallet) return 'instructions'
    return 'choose'
  }, [slug, selectedWallet])

  // Redirect invalid wallet IDs to clean choose URL
  // Also redirect legacy /wallets/add/{wallet-id}/form URLs to /wallets/add/{wallet-id}
  useEffect(() => {
    if (slug && slug.length > 0 && slug[0] !== 'form' && slug[0] !== SAMPLE_WALLET_SLUG && !selectedWallet) {
      router.replace('/wallets/add')
    }
    // Redirect legacy /wallets/add/{wallet-id}/form to /wallets/add/{wallet-id}
    if (slug && slug.length >= 2 && slug[1] === 'form' && selectedWallet) {
      router.replace(`/wallets/add/${selectedWallet.id}`)
    }
  }, [slug, selectedWallet, router])

  // Navigation handlers
  const handleNavigateToChoose = useCallback(() => {
    router.push('/wallets/add')
  }, [router])

  const handleSelectWallet = useCallback((wallet: WalletGuide) => {
    router.push(`/wallets/add/${wallet.id}`)
  }, [router])

  const handleSkipToForm = useCallback(() => {
    router.push('/wallets/add/form')
  }, [router])

  const handleSelectSampleWallet = useCallback(() => {
    router.push(`/wallets/add/${SAMPLE_WALLET_SLUG}`)
  }, [router])

  // Helper to get translated guide steps
  const getGuideSteps = useCallback((walletId: string, t: { raw: (key: string) => unknown }): string[] => {
    try {
      const steps = t.raw(`add.guides.${walletId}.steps`)
      if (Array.isArray(steps)) {
        return steps as string[]
      }
    } catch {
      // Translation not found, fall back to original
    }
    const guide = walletGuides.find(w => w.id === walletId)
    return guide?.steps || []
  }, [])

  return {
    step,
    selectedWallet,
    isBaconWallet,
    baconWallet,
    handleNavigateToChoose,
    handleSelectWallet,
    handleSkipToForm,
    handleSelectSampleWallet,
    getGuideSteps,
  }
}
