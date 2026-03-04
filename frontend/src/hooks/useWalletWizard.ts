import { useMemo, useEffect, useCallback } from "react"
import { useRouter } from "next/navigation"
import { walletGuides, type WalletGuide } from "@/lib/wallet-guides"
import { SAMPLE_WALLETS, isSampleWalletSlug, getSampleWalletForNetwork } from "@/components/add-wallet-form"
import type { WizardStep } from "@/components/wallet-wizard/wizard-breadcrumb"

interface UseWalletWizardOptions {
  slug?: string[]
  network: string
  t: { raw: (key: string) => unknown }
}

interface UseWalletWizardReturn {
  step: WizardStep
  selectedWallet: WalletGuide | null
  isSampleWallet: boolean
  sampleWallet: { name: string; descriptor: string } | undefined
  handleNavigateToChoose: () => void
  handleSelectWallet: (wallet: WalletGuide) => void
  handleSkipToForm: () => void
  handleSelectSampleWallet: (slug: string) => void
  getGuideSteps: (walletId: string) => string[]
}

export function useWalletWizard({ slug, network, t }: UseWalletWizardOptions): UseWalletWizardReturn {
  const router = useRouter()

  // Check if we're using any sample wallet
  const isSampleWallet = slug?.[0] != null && isSampleWalletSlug(slug[0])

  // Get sample wallet data for current network
  const sampleWallet = useMemo(() => {
    if (!isSampleWallet || !slug?.[0]) return undefined
    return getSampleWalletForNetwork(slug[0], network)
  }, [isSampleWallet, slug, network])

  // Derive selected wallet from URL
  const selectedWallet = useMemo(() => {
    if (!slug || slug.length === 0) return null
    if (slug[0] === 'form' || isSampleWalletSlug(slug[0])) return null
    return walletGuides.find(w => w.id === slug[0]) || null
  }, [slug])

  // Derive current step from URL
  const step: WizardStep = useMemo(() => {
    if (!slug || slug.length === 0) return 'choose'
    if (slug[0] === 'form' || isSampleWalletSlug(slug[0])) return 'form'
    if (selectedWallet) return 'instructions'
    return 'choose'
  }, [slug, selectedWallet])

  // Redirect invalid wallet IDs to clean choose URL
  // Also redirect legacy /wallets/add/{wallet-id}/form URLs to /wallets/add/{wallet-id}
  useEffect(() => {
    if (slug && slug.length > 0 && slug[0] !== 'form' && !isSampleWalletSlug(slug[0]) && !selectedWallet) {
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

  const handleSelectSampleWallet = useCallback((sampleSlug: string) => {
    router.push(`/wallets/add/${sampleSlug}`)
  }, [router])

  // Helper to get translated guide steps
  const getGuideSteps = useCallback((walletId: string): string[] => {
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
  }, [t])

  return {
    step,
    selectedWallet,
    isSampleWallet,
    sampleWallet,
    handleNavigateToChoose,
    handleSelectWallet,
    handleSkipToForm,
    handleSelectSampleWallet,
    getGuideSteps,
  }
}
