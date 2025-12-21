export interface WalletGuide {
  id: string
  name: string
  logoSmall: string
  logoLarge: string
  type: 'software' | 'hardware'
  description: string
  steps: string[]
  outputType: 'descriptor' | 'xpub' | 'both'
}

export const walletGuides: WalletGuide[] = [
  // Software Wallets
  {
    id: 'sparrow',
    name: 'Sparrow',
    logoSmall: '/images/wallets/sparrow-small.svg',
    logoLarge: '/images/wallets/sparrow-large.svg',
    type: 'software',
    description: 'Desktop wallet for Bitcoin with full descriptor support',
    outputType: 'descriptor',
    steps: [
      'Open your wallet in Sparrow',
      'Go to the Settings tab',
      'Click "Show" next to Output Descriptor',
      'Copy the entire descriptor string (starts with wpkh, wsh, or tr)',
    ],
  },
  {
    id: 'bluewallet',
    name: 'BlueWallet',
    logoSmall: '/images/wallets/bluewallet-small.svg',
    logoLarge: '/images/wallets/bluewallet-large.svg',
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
  },
  {
    id: 'electrum',
    name: 'Electrum',
    logoSmall: '/images/wallets/electrum-small.svg',
    logoLarge: '/images/wallets/electrum-large.svg',
    type: 'software',
    description: 'Lightweight desktop Bitcoin wallet',
    outputType: 'xpub',
    steps: [
      'Open your wallet in Electrum',
      'Go to Wallet menu → Information',
      'Find the "Master Public Key" section',
      'Copy the zpub, ypub, or xpub string',
    ],
  },
  // Hardware Wallets
  {
    id: 'coldcard',
    name: 'ColdCard',
    logoSmall: '/images/wallets/coldcard-small.svg',
    logoLarge: '/images/wallets/coldcard-large.svg',
    type: 'hardware',
    description: 'Air-gapped hardware wallet',
    outputType: 'both',
    steps: [
      'Power up your ColdCard and enter your PIN',
      'Go to Advanced/Tools → Export Wallet',
      'Select "Generic JSON" format',
      'Export to your SD card',
      'Open the JSON file on your computer',
      'Copy the descriptor or xpub from the file',
    ],
  },
  {
    id: 'ledger',
    name: 'Ledger',
    logoSmall: '/images/wallets/ledger-small.svg',
    logoLarge: '/images/wallets/ledger-large.svg',
    type: 'hardware',
    description: 'Popular hardware wallet',
    outputType: 'xpub',
    steps: [
      'Connect your Ledger device and unlock it',
      'Open the Bitcoin app on your Ledger',
      'Open the Ledger Wallet app on your computer',
      'Go to your Bitcoin account → click the wrench icon',
      'Click "Advanced logs"',
      'Copy the xpub string from the logs',
    ],
  },
  {
    id: 'trezor',
    name: 'Trezor',
    logoSmall: '/images/wallets/trezor-small.svg',
    logoLarge: '/images/wallets/trezor-large.svg',
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
  },
]

export const softwareWallets = walletGuides.filter((w) => w.type === 'software')
export const hardwareWallets = walletGuides.filter((w) => w.type === 'hardware')
