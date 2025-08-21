There seems to be a bug when we do a deep scan. It might be related to the deep scan but I’m not sure. Case: I have this wallet where there is only one address currently with funds, and it's like index 256 or something. It scans. It looks great. Then when I'm waiting on the wallets page, after syncing, it shows up with the correct balance. However, the last activity date is actually the oldest instead of the newest. That's one thing that is incorrect. But if I reload the page or go into the wallet page and go back again to the wallets, then the balance is zero and the last activity date is actually correct. Then it shows the date of the newest transactions.


Please check in the database if the balance is actually zero there as well.


BACKEND LOG:




backend-1  | Creating wallet from multipath descriptor:
backend-1  |   Name: BPISS
backend-1  |   Input descriptor: wpkh([d5680e73/84h/0h/0h]xpub6BgvxAVDid2vrJ8w339QMc3VCNGyVjEQevWNmJ1HvKi8TMHtGN8oMMvpUVRLBefqxt3s6uYC3Q1SrwQqxH97pLvmSTEoyQFyXk4EyKocfhM/<0;1>/*)#d65vh9re
backend-1  |   Stripped key origin: wpkh([d5680e73/84h/0h/0h]xpub6BgvxAVDid2vrJ8w339QMc3VCNGyVjEQevWNmJ1HvKi8TMHtGN8oMMvpUVRLBefqxt3s6uYC3Q1SrwQqxH97pLvmSTEoyQFyXk4EyKocfhM/<0;1>/*) -> wpkh(xpub6BgvxAVDid2vrJ8w339QMc3VCNGyVjEQevWNmJ1HvKi8TMHtGN8oMMvpUVRLBefqxt3s6uYC3Q1SrwQqxH97pLvmSTEoyQFyXk4EyKocfhM/<0;1>/*)
backend-1  |   Final normalized descriptor: wpkh(xpub6BgvxAVDid2vrJ8w339QMc3VCNGyVjEQevWNmJ1HvKi8TMHtGN8oMMvpUVRLBefqxt3s6uYC3Q1SrwQqxH97pLvmSTEoyQFyXk4EyKocfhM/<0;1>/*)#l6refrr3
backend-1  |   Receive descriptor: wpkh(xpub6BgvxAVDid2vrJ8w339QMc3VCNGyVjEQevWNmJ1HvKi8TMHtGN8oMMvpUVRLBefqxt3s6uYC3Q1SrwQqxH97pLvmSTEoyQFyXk4EyKocfhM/0/*)#yawyew4x
backend-1  |   Change descriptor: wpkh(xpub6BgvxAVDid2vrJ8w339QMc3VCNGyVjEQevWNmJ1HvKi8TMHtGN8oMMvpUVRLBefqxt3s6uYC3Q1SrwQqxH97pLvmSTEoyQFyXk4EyKocfhM/1/*)#4ft9ym97
backend-1  |   Wallet filename: l6refrr3.sqlite
backend-1  |   Wallet file path: /app/data/mainnet/wallets/l6refrr3.sqlite
backend-1  |   Metadata saved with checksum: l6refrr3
backend-1  | Received preferred_language from frontend: Some("nb-NO")
backend-1  | Starting background wallet creation for checksum: l6refrr3
backend-1  | Mapping language 'nb-NO' to Norwegian
backend-1  | Auto-created contact 70811f59-fc8d-4883-8267-bec65d9ed98e for user 99ee0cc9-61b7-4d9c-abae-68e09a7c937e in wallet l6refrr3
backend-1  | Syncing with electrum...
backend-1  | Wallet balance before syncing: 0 BTC
backend-1  | Initial address revelation:
backend-1  |   Revealed 51 external addresses
backend-1  |   Revealed 51 internal addresses
backend-1  |
backend-1  | Scan iteration 1
backend-1  |
backend-1  | Scanning keychain [External] 0   1   2   3   4   5   6   7   8   9   10  11  12  13  14  15  16  17  18  19
backend-1  | Scanning keychain [Internal] 0   1   2   3   4   5   6   7   8   9   10  11  12  13  14  15  16  17  18  19
backend-1  | Stop gap satisfied for both keychains
backend-1  | Wallet balance after syncing: 0 BTC
backend-1  | Total addresses revealed - External: 51, Internal: 51
backend-1  | No funds found in initial scan, starting deep scan...
backend-1  | Deep scan batch 1: checking addresses up to index 100
backend-1  |   Revealed 50 external, 50 internal addresses (total: 101 each)
backend-1  |   Need more external addresses: highest used=94, current revealed=101, need=114
backend-1  |   Revealed 14 new external addresses
backend-1  | New addresses revealed, performing additional sync...
backend-1  | 📂 Loaded 1 wallets from disk
backend-1  |   Batch 1 complete - no funds found yet
backend-1  | Deep scan batch 2: checking addresses up to index 200
backend-1  |   Revealed 86 external, 100 internal addresses (total: 201 each)
backend-1  |   Need more external addresses: highest used=188, current revealed=201, need=208
backend-1  |   Revealed 8 new external addresses
backend-1  | New addresses revealed, performing additional sync...
backend-1  |   Batch 2 complete - no funds found yet
backend-1  | Deep scan batch 3: checking addresses up to index 300
backend-1  |   Revealed 92 external, 100 internal addresses (total: 301 each)
backend-1  |   Need more external addresses: highest used=286, current revealed=301, need=306
backend-1  |   Revealed 6 new external addresses
backend-1  | New addresses revealed, performing additional sync...
backend-1  | ✅ Found 67777 sats during deep scan batch 3! Stopping deep scan.
backend-1  | Extracting historical transactions for wallet checksum: l6refrr3
backend-1  | Found 24 historical transactions to process
backend-1  | Current balance: 0.00067777 BTC, Initial balance: 0.00000000 BTC
backend-1  | Historical transaction extraction completed
backend-1  | ✅ Wallet l6refrr3 marked as ready - available for frontend display
backend-1  | Background wallet creation completed for checksum: l6refrr3
backend-1  |   Need more external addresses: highest used=39, current revealed=51, need=59
backend-1  |   Revealed 9 new external addresses
backend-1  | New addresses revealed, performing additional sync...
backend-1  | 📊 Sync summary: 20 cycles completed, 0 with changes, 0 errors