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