# Invalid Network Bug

## Issue
If we add a tpub wallet when running on bitcoin mainnet we get an error:

  Input descriptor: wpkh([9a6a2580/84h/1h/0h]tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi/<0;1>/*)#4laqdwct
  Stripped key origin: wpkh([9a6a2580/84h/1h/0h]tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi/<0;1>/*) -> wpkh(tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi/<0;1>/*)
  Final normalized descriptor: wpkh(tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi/<0;1>/*)#wy8dpdw2
  Receive descriptor: wpkh(tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi/0/*)#305fft0k
  Change descriptor: wpkh(tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi/1/*)#qm3g57lw
[wy8dpdw2] Wallet filename: wy8dpdw2.sqlite
[wy8dpdw2] Wallet file path: ./database/mainnet/wallets/wy8dpdw2.sqlite
[wy8dpdw2] Metadata saved with checksum: wy8dpdw2
Received preferred_language from frontend: Some("nb-NO")
[wy8dpdw2] Starting background wallet creation with stop gap: None
[wy8dpdw2] Background wallet creation failed: Failed to create wallet: Key error: Invalid network
Mapping language 'nb-NO' to Norwegian
Auto-created contact 24d83678-25aa-4a9c-8404-a84455f787a5 for user 31d66c67-5cf0-483d-96a1-e0384d26eb72 in wallet wy8dpdw2

This wallet should actually be disallowed on the POST request, so exit much earlier than this.

## Root Cause Analysis

### Current Flow (Problem)
1. User submits a testnet key (tpub) while backend is running on mainnet
2. API endpoint `create_wallet_non_blocking` in `src/api.rs:415` accepts the request
3. Wallet metadata is saved to database (`src/wallet.rs:131`)
4. Wallet file path is created in `database/mainnet/wallets/`
5. Background task spawned for wallet creation (`src/wallet.rs:140`)
6. BDK's `Wallet::create()` fails with "Key error: Invalid network" (`src/wallet.rs:673-676`)
7. System left in inconsistent state with orphaned metadata

### Key Code Locations
- **API Entry Point**: `src/api.rs:415` - `create_wallet_non_blocking()` function
- **Wallet Creation**: `src/wallet.rs:42` - `WalletCreationService::create_wallet_non_blocking()`
- **Background Task**: `src/wallet.rs:639` - `complete_wallet_creation_with_stop_gap()`
- **BDK Failure Point**: `src/wallet.rs:673-676` - `Wallet::create()` with `.network(network)`
- **XPUB Converter**: `src/xpub_converter.rs` - Currently normalizes keys but doesn't validate network compatibility

### Network Key Prefixes
- **Mainnet Keys**: xpub, ypub, zpub
- **Testnet Keys**: tpub, upub, vpub  
- **Regtest**: Treated same as testnet

### Why It Fails Late
The network validation only happens when BDK tries to create the wallet with the parsed descriptor and specified network. By this time:
- Metadata is already saved
- Wallet checksum is generated
- File paths are created
- Contact may be auto-created

## Solution Required
Network validation must be moved to the API endpoint level, before any database operations or wallet creation starts. This ensures:
1. Fast failure with clear error message
2. No database pollution
3. No inconsistent state
4. Security: validation enforced server-side only 