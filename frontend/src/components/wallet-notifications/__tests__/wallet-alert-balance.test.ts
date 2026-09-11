import type { Wallet } from "@/types"
import { walletAlertBalance } from "../wallet-alert-balance"

const wallet: Wallet = {
  checksum: "abcd",
  name: "Test",
  descriptor: "wpkh(tpub/0/*)",
  wallet_filename: "test",
  hex_color: "#000",
  created_at: "2024-01-01T00:00:00Z",
  balance_total: 50000000,
  last_activity: null,
  status: "ready",
  contact_count: 0,
  is_active: true,
  wallet_type: "descriptor",
  last_synced_at: "2024-01-02T00:00:00Z",
}

describe("walletAlertBalance", () => {
  it("uses the synced total used for alert evaluation", () => {
    expect(walletAlertBalance(wallet)).toEqual({
      status: "ready",
      sats: 50000000,
      fiat: undefined,
      fiatCurrency: undefined,
    })
  })

  it("does not treat a never-synced pending wallet as a zero balance", () => {
    expect(walletAlertBalance({
      ...wallet,
      status: "pending",
      last_synced_at: null,
      balance_total: 0,
    })).toEqual({ status: "unavailable" })
  })

  it("treats failed wallets as unavailable", () => {
    expect(walletAlertBalance({ ...wallet, status: "failed" })).toEqual({ status: "unavailable" })
    expect(walletAlertBalance(null)).toEqual({ status: "unavailable" })
  })
})
