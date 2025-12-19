export interface WalletGuide {
  id: string
  name: string
  logo: string
  type: 'software' | 'hardware'
  description: string
  steps: string[]
  outputType: 'descriptor' | 'xpub' | 'both'
  notes?: string
}

export const walletGuides: WalletGuide[] = [
  // Software Wallets
  {
    id: 'sparrow',
    name: 'Sparrow',
    logo: '/images/wallets/sparrow.svg',
    type: 'software',
    description: 'Desktop wallet for Bitcoin with full descriptor support',
    outputType: 'descriptor',
    steps: [
      'Open your wallet in Sparrow',
      'Go to the Settings tab',
      'Click "Show" next to Output Descriptor',
      'Copy the entire descriptor string (starts with wpkh, wsh, or tr)',
    ],
    notes: 'Sparrow provides full output descriptors which include all the information Canary needs.',
  },
  {
    id: 'bluewallet',
    name: 'BlueWallet',
    logo: '/images/wallets/bluewallet.svg',
    type: 'software',
    description: 'Mobile Bitcoin wallet for iOS and Android',
    outputType: 'xpub',
    steps: [
      'Open your wallet in BlueWallet',
      'Tap the three dots menu (⋯) in the top right',
      'Select "Export/Backup"',
      'Tap "Export Wallet"',
      'Copy the XPUB string (starts with xpub, ypub, or zpub)',
    ],
    notes: 'BlueWallet exports XPUBs. Canary will automatically detect the address type from the prefix.',
  },
  {
    id: 'electrum',
    name: 'Electrum',
    logo: '/images/wallets/electrum.svg',
    type: 'software',
    description: 'Lightweight desktop Bitcoin wallet',
    outputType: 'xpub',
    steps: [
      'Open your wallet in Electrum',
      'Go to Wallet menu → Information',
      'Find the "Master Public Key" section',
      'Copy the zpub, ypub, or xpub string',
    ],
    notes: 'For SegWit wallets, Electrum shows a zpub. For legacy wallets, it shows an xpub.',
  },
  // Hardware Wallets
  {
    id: 'coldcard',
    name: 'ColdCard',
    logo: '/images/wallets/coldcard.svg',
    type: 'hardware',
    description: 'Air-gapped hardware wallet',
    outputType: 'both',
    steps: [
      'Insert your ColdCard and enter your PIN',
      'Go to Advanced/Tools → Export Wallet',
      'Select "Generic JSON" format',
      'Export to your SD card',
      'Open the JSON file on your computer',
      'Copy the descriptor or xpub from the file',
    ],
    notes: 'ColdCard can export full descriptors. The JSON file contains both the descriptor and individual XPUBs.',
  },
  {
    id: 'ledger',
    name: 'Ledger',
    logo: '/images/wallets/ledger.svg',
    type: 'hardware',
    description: 'Popular hardware wallet',
    outputType: 'xpub',
    steps: [
      'Connect your Ledger device and unlock it',
      'Open the Bitcoin app on your Ledger',
      'Open Ledger Live on your computer',
      'Go to your Bitcoin account → click the wrench icon',
      'Click "Advanced logs"',
      'Copy the xpub string from the logs',
    ],
    notes: 'Alternatively, you can use Sparrow Wallet connected to your Ledger for easier descriptor export.',
  },
  {
    id: 'trezor',
    name: 'Trezor',
    logo: '/images/wallets/trezor.svg',
    type: 'hardware',
    description: 'Open-source hardware wallet',
    outputType: 'xpub',
    steps: [
      'Connect your Trezor and go to Trezor Suite',
      'Open your Bitcoin account',
      'Click on the account name to open settings',
      'Click "Show public key (XPUB)"',
      'Confirm on your Trezor device',
      'Copy the XPUB string',
    ],
    notes: 'For an easier experience with full descriptor support, you can connect your Trezor to Sparrow Wallet.',
  },
]

export const softwareWallets = walletGuides.filter((w) => w.type === 'software')
export const hardwareWallets = walletGuides.filter((w) => w.type === 'hardware')
