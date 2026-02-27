#[cfg(test)]
mod tests {
    use crate::xpub_converter::XpubConverter;
    use bdk_wallet::bitcoin::Network;

    /// Derive a regtest address of the given script type from the test tpub.
    /// Uses BDK's descriptor parsing to derive real, valid addresses.
    fn derive_regtest_address(script_type: &str, index: u32) -> String {
        use bdk_wallet::keys::DescriptorPublicKey;
        use miniscript::descriptor::Descriptor;

        let tpub = "tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5";

        let desc_str = match script_type {
            "p2pkh" => format!("pkh({}/0/*)", tpub),
            "p2sh" => format!("sh(wpkh({}/0/*))", tpub),
            "p2wpkh" => format!("wpkh({}/0/*)", tpub),
            "p2tr" => format!("tr({}/0/*)", tpub),
            _ => panic!("unsupported script type: {}", script_type),
        };

        let desc: Descriptor<DescriptorPublicKey> = desc_str.parse().unwrap();
        let derived = desc
            .at_derivation_index(index)
            .expect("derivation should succeed");
        let addr = derived
            .address(Network::Regtest)
            .expect("address derivation should succeed");
        addr.to_string()
    }

    // =========================================================================
    // is_bitcoin_address()
    // =========================================================================

    #[test]
    fn test_is_bitcoin_address_regtest_p2pkh() {
        let addr = derive_regtest_address("p2pkh", 0);
        assert!(
            XpubConverter::is_bitcoin_address(&addr),
            "P2PKH regtest address '{}' should be recognised",
            addr
        );
    }

    #[test]
    fn test_is_bitcoin_address_regtest_p2sh() {
        let addr = derive_regtest_address("p2sh", 0);
        assert!(
            XpubConverter::is_bitcoin_address(&addr),
            "P2SH regtest address '{}' should be recognised",
            addr
        );
    }

    #[test]
    fn test_is_bitcoin_address_regtest_p2wpkh() {
        let addr = derive_regtest_address("p2wpkh", 0);
        assert!(
            addr.starts_with("bcrt1q"),
            "P2WPKH should start with bcrt1q, got: {}",
            addr
        );
        assert!(
            XpubConverter::is_bitcoin_address(&addr),
            "P2WPKH regtest address '{}' should be recognised",
            addr
        );
    }

    #[test]
    fn test_is_bitcoin_address_regtest_p2tr() {
        let addr = derive_regtest_address("p2tr", 0);
        assert!(
            addr.starts_with("bcrt1p"),
            "P2TR should start with bcrt1p, got: {}",
            addr
        );
        assert!(
            XpubConverter::is_bitcoin_address(&addr),
            "P2TR regtest address '{}' should be recognised",
            addr
        );
    }

    #[test]
    fn test_is_bitcoin_address_rejects_non_addresses() {
        // XPUB
        assert!(
            !XpubConverter::is_bitcoin_address(
                "tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5"
            ),
            "XPUB should not be recognised as an address"
        );

        // Descriptor
        assert!(
            !XpubConverter::is_bitcoin_address(
                "wpkh(tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)"
            ),
            "Descriptor should not be recognised as an address"
        );

        // Garbage
        assert!(
            !XpubConverter::is_bitcoin_address("hello world"),
            "Garbage string should not be recognised as an address"
        );
        assert!(
            !XpubConverter::is_bitcoin_address(""),
            "Empty string should not be recognised as an address"
        );
    }

    // =========================================================================
    // validate_address_network()
    // =========================================================================

    #[test]
    fn test_validate_address_network_accepts_regtest() {
        let addr = derive_regtest_address("p2wpkh", 1);
        let result = XpubConverter::validate_address_network(&addr, Network::Regtest);
        assert!(
            result.is_ok(),
            "Regtest address should be valid on regtest: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_validate_address_network_rejects_mainnet_on_regtest() {
        // Use a well-known mainnet address (Satoshi's genesis coinbase)
        let mainnet_addr = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa";
        let result = XpubConverter::validate_address_network(mainnet_addr, Network::Regtest);
        assert!(
            result.is_err(),
            "Mainnet address should be rejected on regtest"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("regtest"),
            "Error should mention regtest: {}",
            err_msg
        );
    }

    #[test]
    fn test_validate_address_network_rejects_invalid_string() {
        let result = XpubConverter::validate_address_network("not-an-address", Network::Regtest);
        assert!(
            result.is_err(),
            "Invalid string should be rejected as an address"
        );
    }

    // =========================================================================
    // address_to_descriptor()
    // =========================================================================

    #[test]
    fn test_address_to_descriptor_format() {
        let addr = derive_regtest_address("p2wpkh", 2);
        let result = XpubConverter::address_to_descriptor(&addr).unwrap();

        assert!(
            result.starts_with(&format!("addr({})", addr)),
            "Descriptor should start with addr(<address>), got: {}",
            result
        );
        assert!(
            result.contains('#'),
            "Descriptor should contain a checksum separator: {}",
            result
        );

        // Checksum should be 8 characters after '#'
        let parts: Vec<&str> = result.splitn(2, '#').collect();
        assert_eq!(parts.len(), 2, "Should have exactly one '#' separator");
        assert_eq!(
            parts[1].len(),
            8,
            "Checksum should be 8 characters, got: '{}'",
            parts[1]
        );
    }

    #[test]
    fn test_address_to_descriptor_deterministic() {
        let addr = derive_regtest_address("p2tr", 3);
        let result1 = XpubConverter::address_to_descriptor(&addr).unwrap();
        let result2 = XpubConverter::address_to_descriptor(&addr).unwrap();
        assert_eq!(
            result1, result2,
            "Same address should always produce the same descriptor"
        );
    }
}
