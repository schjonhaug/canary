import { redirect } from "next/navigation"

export default async function WalletDetailRedirect({
  params,
}: {
  params: Promise<{ checksum: string }>
}) {
  const { checksum } = await params
  redirect(`/wallets/${checksum}/transactions`)
}
