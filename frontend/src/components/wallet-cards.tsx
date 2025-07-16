"use client"

import { useEffect, useState } from "react"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Edit, Users } from "lucide-react"
import { DeleteWalletModal } from "./delete-wallet-modal"
import { EditWalletModal } from "./edit-wallet-modal"
import { extractChecksum, checksumToHexColor } from "@/lib/utils"

interface Wallet {
  id: number
  name: string
  descriptor: string
  wallet_filename: string
  created_at: string
  balance_total: number | null
  last_activity: string | null
  contact_count: number | null
}

interface WalletCardsProps {
  selectedWalletId: number | null
  onSelectWallet: (walletId: number | null) => void
  wallets: Wallet[]
  isConnected: boolean
  error: string | null
  lastUpdate: number | null
}

export function WalletCards({ selectedWalletId, onSelectWallet, wallets, isConnected, error, lastUpdate }: WalletCardsProps) {
  const [hasReceivedData, setHasReceivedData] = useState(false)
  const [walletToDelete, setWalletToDelete] = useState<Wallet | null>(null)
  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false)
  const [walletToEdit, setWalletToEdit] = useState<Wallet | null>(null)
  const [isEditModalOpen, setIsEditModalOpen] = useState(false)

  // Track when we've received data for the first time
  useEffect(() => {
    if (lastUpdate !== null) {
      setHasReceivedData(true)
    }
  }, [lastUpdate])

  const formatBalance = (sats: number | null) => {
    if (sats === null) return "0.00000000 BTC"
    const btc = sats / 100_000_000
    return `${btc.toLocaleString(undefined, { 
      minimumFractionDigits: 8, 
      maximumFractionDigits: 8 
    })} BTC`
  }


  const handleWalletClick = (walletId: number) => {
    if (selectedWalletId === walletId) {
      onSelectWallet(null) // Deselect if already selected
    } else {
      onSelectWallet(walletId)
    }
  }

  const handleEditClick = (wallet: Wallet, event: React.MouseEvent) => {
    event.stopPropagation() // Prevent wallet selection when clicking edit
    setWalletToEdit(wallet)
    setIsEditModalOpen(true)
  }

  const handleDeleteConfirm = async (walletId: number) => {
    const baseUrl = process.env.NEXT_PUBLIC_API_URL || ''
    const response = await fetch(`${baseUrl}/api/wallets/${walletId}`, {
      method: 'DELETE',
    })

    if (!response.ok) {
      if (response.status === 404) {
        throw new Error('Wallet not found')
      }
      throw new Error(`Delete failed: ${response.status}`)
    }

    // Wallet will be removed from state automatically via SSE
    
    // Clear selection if the deleted wallet was selected
    if (selectedWalletId === walletId) {
      onSelectWallet(null)
    }
  }

  const handleDeleteModalClose = () => {
    setIsDeleteModalOpen(false)
    setWalletToDelete(null)
  }

  const handleEditModalClose = () => {
    setIsEditModalOpen(false)
    setWalletToEdit(null)
  }

  const handleDeleteFromEdit = (wallet: Wallet) => {
    // Close edit modal and open delete modal
    setIsEditModalOpen(false)
    setWalletToEdit(null)
    setWalletToDelete(wallet)
    setIsDeleteModalOpen(true)
  }


  const handleWalletUpdated = () => {
    // Wallet list will be updated automatically via SSE
    setIsEditModalOpen(false)
  }

  if (!hasReceivedData) {
    return (
      <div className="space-y-4">
        {/* Individual Wallet Card Skeletons */}
        <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
          {[1, 2, 3].map((i) => (
            <Card key={i} className="hover:shadow-md hover:bg-gray-50/50">
              <CardHeader className="pb-3 relative">
                <div className="flex items-center justify-between">
                  <Skeleton className="h-6 w-32" />
                  <div className="flex items-center gap-2">
                    <Skeleton className="h-8 w-8 rounded-md" />
                    <Skeleton className="h-6 w-16" />
                  </div>
                </div>
                <Skeleton className="h-4 w-24" />
              </CardHeader>
              <CardContent>
                <div className="space-y-2">
                  <div>
                    <Skeleton className="h-4 w-16 mb-1" />
                    <Skeleton className="h-6 w-40" />
                  </div>
                  <div className="flex justify-between items-center">
                    <Skeleton className="h-3 w-32" />
                    <div className="flex items-center gap-1">
                      <Skeleton className="h-3 w-3" />
                      <Skeleton className="h-3 w-4" />
                    </div>
                  </div>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Error</CardTitle>
          <CardDescription className="text-destructive">
            Failed to load wallets: {error}
          </CardDescription>
        </CardHeader>
      </Card>
    )
  }

  if (wallets.length === 0) {
    return (
      <div className="space-y-4">
        <Card>
          <CardHeader>
            <CardTitle>No Wallets</CardTitle>
            <CardDescription>
              No wallets found. Use the "Create Wallet" button in the header to get started.
            </CardDescription>
          </CardHeader>
        </Card>

      </div>
    )
  }

  return (
    <div className="space-y-4">
      {/* Individual Wallet Cards */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
        {wallets.map((wallet) => {
          const isSelected = selectedWalletId === wallet.id
          return (
            <Card 
              key={wallet.id} 
              className={`cursor-pointer transition-all duration-200 ${
                isSelected 
                  ? "ring-2 ring-accent bg-accent/5 shadow-lg" 
                  : "hover:shadow-md hover:bg-muted/50"
              }`}
              onClick={() => handleWalletClick(wallet.id)}
            >
              <CardHeader className="pb-3 relative">
                <CardTitle className="text-lg truncate pr-20" title={wallet.name}>
                  {wallet.name}
                </CardTitle>
                <div className="absolute top-2 right-2 flex items-center gap-2">
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-8 w-8 p-0 hover:bg-accent/10 hover:text-accent"
                    onClick={(e) => handleEditClick(wallet, e)}
                    title="Edit wallet"
                  >
                    <Edit className="h-4 w-4" />
                  </Button>
                </div>
                <div 
                  className="absolute top-2 right-12 w-6 h-6 cursor-help"
                  title={`Checksum: #${extractChecksum(wallet.descriptor)}`}
                >
                  <svg width="24" height="24" viewBox="0 0 691 562" fill="none" xmlns="http://www.w3.org/2000/svg">
                    <path d="M27.6423 524.065C43.4144 510.338 77.6898 481.872 99.0102 464.793C154.77 420.125 183.537 397.337 185.476 396.296C186.595 395.696 192.01 394.653 197.51 393.979C203.01 393.304 211.906 391.873 217.278 390.798C226.244 389.003 227.518 388.964 232.778 390.32C241.758 392.634 260.38 395.684 273.37 396.967C290.901 398.699 327.209 397.651 342.941 394.959C395.285 386.001 438.048 365.024 470.51 332.382C493.152 309.614 506.01 281.583 506.01 254.992C506.01 247.459 503.04 232.142 499.784 222.888C494.552 208.016 494.51 208.058 494.51 228.191C494.51 242.341 494.107 248.056 492.697 253.888C485.57 283.361 472.182 305.425 449.271 325.455C421.258 349.946 385.926 365.698 341.51 373.499C328.267 375.825 323.231 376.178 298.51 376.512C280.083 376.76 271.194 376.56 272.51 375.925C273.61 375.395 279.01 373.329 284.51 371.335C307.085 363.15 331.897 349.387 349.101 335.507C357.089 329.063 373.22 314.287 371.51 314.982C370.96 315.205 363.31 319.051 354.51 323.529C287.789 357.476 221.128 375.617 149.01 379.455C137.964 380.043 136.685 379.948 138.01 378.639C138.835 377.824 142.685 374.284 146.566 370.773C150.446 367.261 162.146 356.504 172.566 346.867C193.453 327.549 210.976 311.833 239.438 286.888C249.793 277.813 263.093 266.113 268.995 260.888C287.643 244.377 391.253 153.606 397.279 148.501C409.08 138.502 415.358 127.854 422.107 106.388C431.469 76.6121 441.158 61.0375 460.635 44.4545C481.145 26.9927 503.943 19.0775 530.51 20.1949C544.962 20.8027 553.672 22.8234 565.603 28.3357C583.889 36.7844 596.178 47.6651 608.635 66.4356L616.51 78.3013L635.845 90.8279L655.179 103.354L639.845 110.837C626.015 117.585 623.764 119.059 616.907 125.854C608.258 134.424 603.374 143.264 599.969 156.51C596.691 169.26 596.465 179.87 599.001 201.888C603.668 242.401 598.951 274.24 582.879 310.706C571.456 336.623 557.46 357.113 536.171 379.085C492.731 423.918 441.4 446.948 376.251 450.834C367.088 451.38 355.495 451.416 348.51 450.919C341.91 450.45 336.244 450.364 335.918 450.727C335.593 451.091 334.076 456.338 332.547 462.388C331.019 468.438 329.472 473.942 329.111 474.618C328.65 475.48 326.746 474.636 322.745 471.798C318.525 468.804 315.062 464.899 309.462 456.818C305.296 450.807 301.693 445.888 301.455 445.888C301.218 445.888 295.058 444.733 287.767 443.321C270.796 440.035 253.361 435.657 239.8 431.275L229.09 427.814L223.827 430.481C218.764 433.047 212.085 437.182 161.51 469.07C129.858 489.027 100.165 506.786 89.4685 512.157C72.4442 520.704 54.5645 525.477 34.8921 526.725L23.7744 527.431L27.6423 524.065ZM562.51 103.612C574.226 95.7131 575.046 81.3709 564.321 71.9305C554.279 63.091 538.612 66.0021 532.258 77.8881C527.206 87.3393 531.141 99.3667 541.01 104.638C547.263 107.978 556.702 107.528 562.51 103.612Z" fill={checksumToHexColor(extractChecksum(wallet.descriptor))}/>
                    <path d="M309.76 561.139C286.514 560.893 283.01 560.655 283.01 559.322C283.01 556.692 285.76 552.315 289.445 549.079C295.361 543.885 301.141 542.694 323.01 542.163C333.735 541.903 342.821 541.397 343.2 541.039C343.864 540.412 339.108 533.183 306.517 485.282L291.524 463.246L281.415 461.536C269.491 459.518 251.084 454.969 240.591 451.447C233.716 449.139 232.805 449.049 229.193 450.324C227.034 451.085 214.522 458.435 201.389 466.657C154.528 495.993 107.838 524.069 95.1356 530.552C85.069 535.689 72.4537 539.907 57.8898 543.006C49.5144 544.788 44.0346 545.231 29.5102 545.301C13.2007 545.38 11.1432 545.195 7.60184 543.338C0.296639 539.505 -2.28762 529.592 2.26021 522.847C7.0043 515.811 40.4476 487.359 103.732 436.52L149.953 399.388L134.232 398.715C119.129 398.068 100.266 395.69 97.3659 394.067C94.6939 392.572 102.214 384.984 144.51 346.496C208.218 288.526 244.591 256.212 340.01 172.815C366.135 149.982 389.227 129.295 391.325 126.844C396.169 121.186 400.05 112.94 403.542 100.888C411.369 73.8784 422.843 53.9912 440.597 36.6662C464.923 12.9281 493.423 0.888062 525.286 0.888062C550.05 0.888062 568.755 6.43741 589.546 19.953C600.387 27.0003 618.254 45.0261 625.368 56.0922C629.732 62.8813 632.166 65.5661 636.433 68.2953C645.245 73.9327 678.001 96.8057 684.26 101.693C687.423 104.163 690.01 106.549 690.01 106.997C690.01 108.466 685.598 112.02 680.607 114.571C677.911 115.949 668.801 119.952 660.363 123.465C638.926 132.392 633.781 135.17 629.566 140.094C618.145 153.436 614.636 173.307 618.538 202.55C620.535 217.51 620.53 241.876 618.528 257.113C613.593 294.68 596.81 334.5 572.07 367.343C562.778 379.678 540.799 402.428 528.591 412.348C495.615 439.142 457.58 457.298 418.683 464.81C413.279 465.854 408.637 466.928 408.368 467.197C407.97 467.595 439.727 513.019 453.083 531.156L456.51 535.81L475.51 535.857C496.165 535.907 504.821 536.891 512.077 540.012C517.938 542.534 521.497 546.184 523.427 551.652L524.922 555.888H493.966H463.01V558.888V561.888L399.76 561.655C364.973 561.527 324.473 561.295 309.76 561.139ZM427.154 533.888C423.145 528.222 387.913 479.977 382.493 472.73L380.51 470.08L356.342 469.734C343.05 469.544 331.939 469.152 331.651 468.863C331.068 468.279 334.935 451.771 335.899 450.727C336.235 450.364 341.91 450.45 348.51 450.919C369.816 452.434 397.148 449.989 422.859 444.268C464.731 434.952 503.835 412.457 536.171 379.085C557.46 357.113 571.456 336.623 582.879 310.706C598.951 274.24 603.669 242.401 599.001 201.888C596.465 179.87 596.691 169.26 599.969 156.51C603.374 143.264 608.258 134.424 616.907 125.854C623.764 119.059 626.015 117.585 639.845 110.837L655.179 103.354L635.845 90.8278L616.51 78.3012L608.636 66.4355C596.179 47.665 583.889 36.7843 565.603 28.3356C553.673 22.8233 544.962 20.8026 530.51 20.1948C503.943 19.0774 481.145 26.9927 460.635 44.4544C441.158 61.0374 431.469 76.612 422.107 106.388C415.358 127.854 409.08 138.502 397.279 148.501C391.253 153.606 287.643 244.377 268.995 260.888C263.093 266.113 249.793 277.813 239.438 286.888C210.976 311.833 193.453 327.549 172.566 346.867C162.146 356.504 150.446 367.261 146.566 370.773C142.685 374.284 138.835 377.824 138.01 378.639C136.685 379.948 137.964 380.043 149.01 379.455C221.128 375.617 287.789 357.476 354.51 323.529C363.31 319.051 370.96 315.205 371.51 314.982C373.22 314.287 357.089 329.063 349.102 335.507C331.897 349.387 307.085 363.15 284.51 371.335C279.01 373.329 273.61 375.395 272.51 375.925C271.194 376.56 280.084 376.76 298.51 376.512C323.231 376.178 328.267 375.825 341.51 373.499C385.926 365.698 421.258 349.946 449.271 325.455C472.182 305.425 485.57 283.361 492.697 253.888C494.107 248.056 494.51 242.341 494.51 228.191C494.51 208.058 494.552 208.016 499.784 222.888C503.04 232.142 506.01 247.458 506.01 254.992C506.01 281.583 493.152 309.614 470.51 332.381C438.048 365.024 395.285 386.001 342.941 394.959C327.209 397.651 290.901 398.699 273.37 396.967C260.38 395.684 241.758 392.634 232.778 390.32C227.518 388.964 226.244 389.003 217.278 390.798C211.906 391.873 203.01 393.304 197.51 393.979C192.01 394.653 186.595 395.696 185.476 396.296C183.537 397.337 154.77 420.125 99.0102 464.793C77.6898 481.872 43.4144 510.338 27.6423 524.065L23.7744 527.431L34.8921 526.725C54.5645 525.477 72.4442 520.704 89.4685 512.157C100.165 506.786 129.858 489.027 161.51 469.07C212.085 437.182 218.764 433.047 223.827 430.481L229.09 427.814L239.8 431.275C253.361 435.657 270.796 440.035 287.767 443.321C295.058 444.733 301.218 445.888 301.455 445.888C301.693 445.888 305.305 450.82 309.482 456.848C315.216 465.122 318.505 468.809 322.904 471.894C327.929 475.42 329.945 477.934 337.563 490.185C342.421 497.997 351.682 512.488 358.144 522.388L369.891 540.388H400.822H431.754L427.154 533.888ZM541.01 104.638C531.141 99.3666 527.206 87.3393 532.258 77.8881C538.612 66.002 554.279 63.091 564.321 71.9304C575.046 81.3708 574.226 95.713 562.51 103.612C556.702 107.528 547.263 107.978 541.01 104.638Z" fill="#161812"/>
                  </svg>
                </div>
                <CardDescription className="text-xs text-muted-foreground">
                  Click to {isSelected ? 'deselect' : 'view transactions'}
                </CardDescription>
              </CardHeader>
              <CardContent>
                <div className="space-y-2">
                  <div>
                    <div className="text-sm text-muted-foreground">Balance</div>
                    <div className={`text-xl font-bold font-mono ${
                      isSelected ? "text-accent" : ""
                    }`}>
                      {formatBalance(wallet.balance_total)}
                    </div>
                  </div>
                  <div className="flex justify-between items-center text-xs text-muted-foreground">
                    <span>
                      {wallet.last_activity 
                        ? `Last activity: ${new Date(wallet.last_activity).toLocaleDateString()}` 
                        : "No recent activity"
                      }
                    </span>
                    <div className="flex items-center gap-1">
                      <Users className="h-3 w-3" />
                      <span>{wallet.contact_count || 0}</span>
                    </div>
                  </div>
                </div>
              </CardContent>
            </Card>
          )
        })}
      </div>

      <DeleteWalletModal
        wallet={walletToDelete}
        isOpen={isDeleteModalOpen}
        onClose={handleDeleteModalClose}
        onConfirmDelete={handleDeleteConfirm}
      />

      <EditWalletModal
        wallet={walletToEdit}
        isOpen={isEditModalOpen}
        onClose={handleEditModalClose}
        onWalletUpdated={handleWalletUpdated}
        onDeleteWallet={handleDeleteFromEdit}
      />
    </div>
  )
}