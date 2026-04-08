import { act, renderHook, waitFor } from "@testing-library/react"
import { useWalletDetail } from "../useWalletDetail"

const mockGetWalletDetail = jest.fn()
const mockGetTransactionNotifications = jest.fn()
const mockUseAuth = jest.fn()

jest.mock("../../lib/api", () => ({
  api: {
    getWalletDetail: (...args: unknown[]) => mockGetWalletDetail(...args),
    getTransactionNotifications: (...args: unknown[]) =>
      mockGetTransactionNotifications(...args),
  },
}))

jest.mock("../../contexts/auth-context", () => ({
  useAuth: () => mockUseAuth(),
}))

const wallet = {
  checksum: "wallet-1",
  name: "Wallet One",
  descriptor: "descriptor",
  wallet_filename: "wallet-1.json",
  hex_color: "#000000",
  created_at: "2026-01-01T00:00:00Z",
  balance_total: 0,
  status: "ready",
  contact_count: 0,
  is_active: true,
  wallet_type: "descriptor" as const,
  last_activity: null,
}

const transaction = {
  txid: "tx-1",
  wallet_checksum: wallet.checksum,
  wallet_name: wallet.name,
  transaction_type: "receive" as const,
  amount_sats: 1234,
  fee_sats: null,
  block_height: null,
  first_seen_at: 1_700_000_000,
  confirmed_at: null,
  parent_txid: null,
  transaction_status: "pending",
  replaced_by_txid: null,
  replaced_at: null,
}

function buildWalletDetailResponse(overrides?: {
  timestamp?: number
  transactions?: typeof transaction[]
}) {
  return {
    timestamp: overrides?.timestamp ?? 1_700_000_000,
    wallet,
    transactions: overrides?.transactions ?? [transaction],
    contacts: [],
    balance_alerts: [],
    pagination: {
      page_size: 100,
      next_cursor: null,
      has_more: false,
      applied_since_timestamp: null,
    },
  }
}

describe("useWalletDetail", () => {
  beforeEach(() => {
    jest.useFakeTimers()
    jest.clearAllMocks()
    mockUseAuth.mockReturnValue({
      isAuthenticated: true,
      billingStatus: {
        limits: {
          sync_interval_seconds: 1,
        },
      },
    })
  })

  afterEach(() => {
    jest.clearAllTimers()
    jest.useRealTimers()
  })

  it("maps 404 wallet detail failures from statusCode without marking the app disconnected", async () => {
    mockGetWalletDetail.mockRejectedValue({ statusCode: 404 })

    const { result } = renderHook(() => useWalletDetail(wallet.checksum))

    await waitFor(() => {
      expect(result.current.error).toBe("Wallet not found")
    })

    expect(result.current.isConnected).toBe(true)
  })

  it("maps 403 wallet detail failures from statusCode without marking the app disconnected", async () => {
    mockGetWalletDetail.mockRejectedValue({ statusCode: 403 })

    const { result } = renderHook(() => useWalletDetail(wallet.checksum))

    await waitFor(() => {
      expect(result.current.error).toBe("Access denied to wallet")
    })

    expect(result.current.isConnected).toBe(true)
  })

  it("preserves cached notifications across polls when no transactions changed", async () => {
    mockGetWalletDetail
      .mockResolvedValueOnce(buildWalletDetailResponse())
      .mockResolvedValueOnce(
        buildWalletDetailResponse({
          timestamp: 1_700_000_100,
          transactions: [],
        })
      )
    mockGetTransactionNotifications.mockResolvedValue([
      {
        contact_name: "Alice",
        provider_name: "email",
        status: "sent",
        error_message: null,
        created_at: "2026-01-01T00:00:00Z",
        notification_type: "pending",
      },
    ])

    const { result } = renderHook(() => useWalletDetail(wallet.checksum))

    await waitFor(() => {
      expect(result.current.transactions).toHaveLength(1)
    })

    await act(async () => {
      await result.current.loadTransactionNotifications(wallet.checksum, transaction.txid)
    })

    await waitFor(() => {
      expect(
        result.current.transactionNotifications[`${wallet.checksum}:${transaction.txid}`]
      ).toHaveLength(1)
    })

    expect(mockGetTransactionNotifications).toHaveBeenCalledTimes(1)

    await act(async () => {
      jest.advanceTimersByTime(1000)
      await Promise.resolve()
    })

    await waitFor(() => {
      expect(mockGetWalletDetail).toHaveBeenCalledTimes(2)
    })

    expect(
      result.current.transactionNotifications[`${wallet.checksum}:${transaction.txid}`]
    ).toHaveLength(1)

    await act(async () => {
      await result.current.loadTransactionNotifications(wallet.checksum, transaction.txid)
    })

    expect(mockGetTransactionNotifications).toHaveBeenCalledTimes(1)
  })

  it("drops stale cached notification state for updated transactions during polling", async () => {
    mockGetWalletDetail
      .mockResolvedValueOnce(buildWalletDetailResponse())
      .mockResolvedValueOnce(
        buildWalletDetailResponse({
          timestamp: 1_700_000_100,
          transactions: [{ ...transaction, confirmed_at: 1_700_000_100 }],
        })
      )
    mockGetTransactionNotifications.mockResolvedValue([
      {
        contact_name: "Alice",
        provider_name: "email",
        status: "sent",
        error_message: null,
        created_at: "2026-01-01T00:00:00Z",
        notification_type: "pending",
      },
    ])

    const { result } = renderHook(() => useWalletDetail(wallet.checksum))

    await waitFor(() => {
      expect(result.current.transactions).toHaveLength(1)
    })

    await act(async () => {
      await result.current.loadTransactionNotifications(wallet.checksum, transaction.txid)
    })

    await waitFor(() => {
      expect(result.current.loadingTransactionNotifications[`${wallet.checksum}:${transaction.txid}`]).toBe(false)
    })

    await act(async () => {
      jest.advanceTimersByTime(1000)
      await Promise.resolve()
    })

    await waitFor(() => {
      expect(mockGetWalletDetail).toHaveBeenCalledTimes(2)
    })

    expect(
      result.current.transactionNotifications[`${wallet.checksum}:${transaction.txid}`]
    ).toBeUndefined()
    expect(
      result.current.loadingTransactionNotifications[`${wallet.checksum}:${transaction.txid}`]
    ).toBeUndefined()
  })
})
