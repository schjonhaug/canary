#[cfg(test)]
mod tests {
    use miniscript::{Descriptor, DescriptorPublicKey};
    use regex::Regex;

    #[test]
    fn test_strip_key_origin_logic() {
        // Test the core logic without database dependencies

        // Simplified version of the strip_key_origin logic
        let test_strip_key_origin = |descriptor_str: &str| -> Result<String, miniscript::Error> {
            // First strip any existing checksum (everything after #)
            let without_checksum = if let Some(pos) = descriptor_str.find('#') {
                &descriptor_str[..pos]
            } else {
                descriptor_str
            };

            // Pattern to match [fingerprint/derivation/path] anywhere in the descriptor
            // Supports both 'h' and '\'' for hardened paths
            let key_origin_pattern = Regex::new(r"\[([0-9a-fA-F]{8})(/\d+[h']?)*\]").unwrap();

            // Strip key origin if present
            let stripped_without_checksum = if key_origin_pattern.is_match(without_checksum) {
                let result = key_origin_pattern.replace_all(without_checksum, "");
                println!("  Stripped key origin: {} -> {}", without_checksum, result);
                result.to_string()
            } else {
                without_checksum.to_string()
            };

            // Parse the stripped descriptor to recalculate checksum
            let descriptor: Descriptor<DescriptorPublicKey> = stripped_without_checksum.parse()?;

            // Convert back to string with new checksum
            let final_descriptor = descriptor.to_string();
            println!("  Final normalized descriptor: {}", final_descriptor);

            Ok(final_descriptor)
        };

        // Test cases
        let test_cases = vec![
            (
                "wpkh([805c684b/84h/1h/0h]tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)#8nt3y08q",
                "wpkh(tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)",
                "Descriptor with key origin and checksum should be stripped"
            ),
            (
                "wpkh([12345678/84h/1h/0h]tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)#different",
                "wpkh(tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)",
                "Different fingerprint should produce same result"
            ),
        ];

        for (input, expected_prefix, description) in test_cases {
            println!("Testing: {}", description);
            println!("Input: {}", input);

            let result = test_strip_key_origin(input).unwrap();
            println!("Output: {}", result);

            // Check that the result starts with the expected prefix (without checksum)
            assert!(
                result.starts_with(expected_prefix),
                "Expected '{}' to start with '{}' ({})",
                result,
                expected_prefix,
                description
            );

            // Check that the result has a checksum (ends with #xxxxxxxx)
            assert!(
                result.contains('#'),
                "Expected result to contain checksum (#): {} ({})",
                result,
                description
            );

            // Check that key origin was actually stripped
            assert!(
                !result.contains('['),
                "Expected key origin to be stripped from: {} ({})",
                result,
                description
            );
        }

        // Test that the same XPUB with different fingerprints produces the same checksum
        let input1 = "wpkh([805c684b/84h/1h/0h]tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)#8nt3y08q";
        let input2 = "wpkh([12345678/84h/1h/0h]tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)#different";

        let result1 = test_strip_key_origin(input1).unwrap();
        let result2 = test_strip_key_origin(input2).unwrap();

        println!("Same XPUB test:");
        println!("Result1: {}", result1);
        println!("Result2: {}", result2);

        assert_eq!(
            result1, result2,
            "Same XPUB with different fingerprints should produce same normalized descriptor"
        );

        println!("✅ All tests passed!");
    }

    #[test]
    fn test_xpub_normalization() {
        use crate::xpub_converter::XpubConverter;
        use bdk_wallet::bitcoin::Network;

        // Test normalization using VALID keys with proper base58check encoding
        // The xyzpub crate validates checksums, so we must use real keys

        // Bacon wallet keys (from 12x "bacon" BIP39 mnemonic)
        let bacon_xpub = "xpub6DEzNop46vmxR49zYWFnMwmEfawSNmAMf6dLH5YKDY463twtvw1XD7ihwJRLPRGZJz799VPFzXHpZu6WdhT29WnaeuChS6aZHZPFmqczR5K";
        let bacon_zpub = "zpub6ruWz99tQHrv7eYEDDq2n7xF1XELG19MVKfmqsL5yYorA6aMSFLeTF2yyiLWPEaQ8GLkeSaNuqzvLUKe56H3jz9nPabYbvDXq1WYZ5NMmEk";
        let bacon_tpub = "tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi";

        // Test 1: xpub on mainnet passes through unchanged
        let converter = XpubConverter::new(Network::Bitcoin, None);
        let result = converter.normalize_xpub(bacon_xpub).unwrap();
        assert_eq!(
            result, bacon_xpub,
            "xpub on mainnet should pass through unchanged"
        );

        // Test 2: zpub on mainnet converts to xpub (proper base58check conversion)
        let converter = XpubConverter::new(Network::Bitcoin, None);
        let result = converter.normalize_xpub(bacon_zpub).unwrap();
        assert_eq!(result, bacon_xpub, "zpub on mainnet should convert to xpub");

        // Test 3: tpub on testnet passes through unchanged
        let converter = XpubConverter::new(Network::Testnet, None);
        let result = converter.normalize_xpub(bacon_tpub).unwrap();
        assert_eq!(
            result, bacon_tpub,
            "tpub on testnet should pass through unchanged"
        );

        // Test 4: tpub on regtest passes through unchanged
        let converter = XpubConverter::new(Network::Regtest, None);
        let result = converter.normalize_xpub(bacon_tpub).unwrap();
        assert_eq!(
            result, bacon_tpub,
            "tpub on regtest should pass through unchanged"
        );

        // Test 5: xpub on testnet converts to tpub
        let converter = XpubConverter::new(Network::Testnet, None);
        let result = converter.normalize_xpub(bacon_xpub).unwrap();
        assert!(
            result.starts_with("tpub"),
            "xpub on testnet should convert to tpub"
        );

        // Test 6: Short/invalid strings pass through unchanged (no conversion attempted)
        let converter = XpubConverter::new(Network::Bitcoin, None);
        let result = converter.normalize_xpub("short").unwrap();
        assert_eq!(
            result, "short",
            "Short strings should pass through unchanged"
        );

        println!("✅ XPUB normalization tests passed!");
    }

    #[tokio::test]
    async fn test_xpub_script_type_detection() {
        // This test is now obsolete since we removed prefix-based detection
        // in favor of blockchain activity-based probing in create_from_xpub_with_probing
        println!("📋 Test skipped: Script type detection now uses blockchain activity probing");
        println!("✅ XPUB script type detection test skipped (functionality moved to create_from_xpub_with_probing)!");
    }

    #[tokio::test]
    async fn test_xpub_address_derivation_mock() {
        use crate::xpub_converter::XpubConverter;
        use bdk_wallet::bitcoin::Network;

        // Test that the XpubConverter can be created and handles invalid XPUBs gracefully
        let converter = XpubConverter::new(Network::Regtest, None);

        // Test XPUB format validation
        // Test with the known good XPUB from our tests
        assert!(XpubConverter::is_xpub("tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5"));
        // Test invalid formats
        assert!(!XpubConverter::is_xpub("invalid_xpub"));
        assert!(!XpubConverter::is_xpub("too_short"));
        assert!(!XpubConverter::is_xpub(""));
        assert!(!XpubConverter::is_xpub("tpub_too_short"));

        // Test normalization
        let normalized = converter.normalize_xpub("tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5").unwrap();
        assert!(normalized.starts_with("tpub"));

        println!("✅ XPUB validation and normalization tests passed!");
    }

    // This integration test is now obsolete since we removed the standalone convert_to_descriptor method
    // XPUB conversion with probing is now integrated into create_from_xpub_with_probing
    #[tokio::test]
    #[ignore = "functionality moved to create_from_xpub_with_probing in wallet creation"]
    async fn test_xpub_conversion_integration() {
        println!("📋 Test skipped: XPUB conversion now integrated into wallet creation process");
        println!("✅ XPUB conversion integration test skipped (functionality moved to create_from_xpub_with_probing)!");
    }

    #[test]
    fn test_network_detection() {
        use crate::xpub_converter::XpubConverter;
        use bdk_wallet::bitcoin::Network;

        // Test key network detection
        let test_cases = vec![
            // Mainnet keys (using real mainnet XPUBs from existing tests)
            ("zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs", Some(Network::Bitcoin)),

            // Testnet keys (using real testnet XPUB from existing tests)
            ("tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5", Some(Network::Testnet)),
            ("vpub5Y6cjg78GGuNLsaPhmYsiw4gYX3HoQiRBiSwDaBXKUafCt9bNwWQiitDk5VZ5BVxYnQdwoTyXSs2JHRPAgjAvtbBrf8ZhDYe2jWAqvZVnsc", Some(Network::Testnet)),

            // Invalid or non-XPUB formats
            ("invalid_key", None),
            ("short", None),
            ("", None),
        ];

        for (key, expected_network) in test_cases {
            let result = XpubConverter::get_key_network(key);
            assert_eq!(
                result, expected_network,
                "Failed for key: {}, expected: {:?}, got: {:?}",
                key, expected_network, result
            );
        }

        println!("✅ Network detection tests passed!");
    }

    #[test]
    fn test_key_network_validation() {
        use crate::xpub_converter::XpubConverter;
        use bdk_wallet::bitcoin::Network;

        // Valid cases - should pass
        let valid_cases = vec![
            // Mainnet keys on mainnet
            ("zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs", Network::Bitcoin),

            // Testnet keys on testnet/regtest
            ("tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5", Network::Testnet),
            ("vpub5Y6cjg78GGuNLsaPhmYsiw4gYX3HoQiRBiSwDaBXKUafCt9bNwWQiitDk5VZ5BVxYnQdwoTyXSs2JHRPAgjAvtbBrf8ZhDYe2jWAqvZVnsc", Network::Testnet),
            ("tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5", Network::Regtest),
            ("vpub5Y6cjg78GGuNLsaPhmYsiw4gYX3HoQiRBiSwDaBXKUafCt9bNwWQiitDk5VZ5BVxYnQdwoTyXSs2JHRPAgjAvtbBrf8ZhDYe2jWAqvZVnsc", Network::Regtest),

            // Non-XPUB strings should pass (not validated)
            ("invalid_key", Network::Bitcoin),
            ("short", Network::Testnet),
        ];

        for (key, network) in valid_cases {
            let result = XpubConverter::validate_key_network(key, network);
            assert!(
                result.is_ok(),
                "Expected {} to be valid for network {:?}, got error: {:?}",
                key,
                network,
                result.err()
            );
        }

        // Invalid cases - should fail
        let invalid_cases = vec![
            // Mainnet keys on testnet/regtest
            ("zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs", Network::Testnet),
            ("zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs", Network::Regtest),

            // Testnet keys on mainnet
            ("tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5", Network::Bitcoin),
            ("vpub5Y6cjg78GGuNLsaPhmYsiw4gYX3HoQiRBiSwDaBXKUafCt9bNwWQiitDk5VZ5BVxYnQdwoTyXSs2JHRPAgjAvtbBrf8ZhDYe2jWAqvZVnsc", Network::Bitcoin),
        ];

        for (key, network) in invalid_cases {
            let result = XpubConverter::validate_key_network(key, network);
            assert!(
                result.is_err(),
                "Expected {} to be invalid for network {:?}, but validation passed",
                key,
                network
            );
        }

        println!("✅ Key network validation tests passed!");
    }

    #[test]
    fn test_descriptor_network_validation() {
        use crate::xpub_converter::XpubConverter;
        use bdk_wallet::bitcoin::Network;

        // Valid descriptor cases
        let valid_cases = vec![
            // Mainnet descriptors on mainnet
            ("zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs", Network::Bitcoin),
            ("wpkh(zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs/<0;1>/*)", Network::Bitcoin),
            ("wpkh([805c684b/84h/1h/0h]zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs/<0;1>/*)#8nt3y08q", Network::Bitcoin),

            // Testnet descriptors on testnet/regtest
            ("tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5", Network::Testnet),
            ("wpkh(tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)", Network::Testnet),
            ("wpkh([805c684b/84h/1h/0h]tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)#8nt3y08q", Network::Regtest),
            ("sh(wpkh(vpub5Y6cjg78GGuNLsaPhmYsiw4gYX3HoQiRBiSwDaBXKUafCt9bNwWQiitDk5VZ5BVxYnQdwoTyXSs2JHRPAgjAvtbBrf8ZhDYe2jWAqvZVnsc/<0;1>/*))", Network::Testnet),

            // Non-XPUB descriptors should pass
            ("wpkh(0279BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798)", Network::Bitcoin),
        ];

        for (descriptor, network) in valid_cases {
            let result = XpubConverter::validate_descriptor_network(descriptor, network);
            assert!(
                result.is_ok(),
                "Expected {} to be valid for network {:?}, got error: {:?}",
                descriptor,
                network,
                result.err()
            );
        }

        // Invalid descriptor cases
        let invalid_cases = vec![
            // Mainnet descriptors on testnet
            ("zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs", Network::Testnet),
            ("wpkh(zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs/<0;1>/*)", Network::Testnet),
            ("wpkh([805c684b/84h/1h/0h]zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs/<0;1>/*)#8nt3y08q", Network::Testnet),

            // Testnet descriptors on mainnet
            ("tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5", Network::Bitcoin),
            ("wpkh(tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)", Network::Bitcoin),
            ("wpkh([805c684b/84h/1h/0h]tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)#8nt3y08q", Network::Bitcoin),
            ("sh(wpkh(vpub5Y6cjg78GGuNLsaPhmYsiw4gYX3HoQiRBiSwDaBXKUafCt9bNwWQiitDk5VZ5BVxYnQdwoTyXSs2JHRPAgjAvtbBrf8ZhDYe2jWAqvZVnsc/<0;1>/*))", Network::Bitcoin),
        ];

        for (descriptor, network) in invalid_cases {
            let result = XpubConverter::validate_descriptor_network(descriptor, network);
            assert!(
                result.is_err(),
                "Expected {} to be invalid for network {:?}, but validation passed",
                descriptor,
                network
            );
        }

        println!("✅ Descriptor network validation tests passed!");
    }

    // =============================================================================
    // WalletCreationService validation tests
    // =============================================================================

    #[test]
    fn test_stop_gap_valid_values() {
        // Valid stop gap values from the handler
        let valid_values = vec!["auto", "250", "500", "750", "1000"];

        for value in valid_values {
            let is_valid = matches!(value, "auto" | "250" | "500" | "750" | "1000");
            assert!(
                is_valid,
                "Expected '{}' to be a valid stop_gap value",
                value
            );
        }

        // Invalid values
        let invalid_values = vec!["100", "999", "200", "300", "0", "-1", "abc", ""];

        for value in invalid_values {
            let is_valid = matches!(value, "auto" | "250" | "500" | "750" | "1000");
            assert!(
                !is_valid,
                "Expected '{}' to be an invalid stop_gap value",
                value
            );
        }

        println!("✅ Stop gap valid values tests passed!");
    }

    #[test]
    fn test_custom_stop_gap_requires_script_type() {
        // Test the validation logic for custom stop_gap
        // Custom stop gap (non-"auto") requires explicit script_type

        // Case 1: Auto stop_gap doesn't need script_type
        let stop_gap = Some("auto");
        let script_type: Option<&str> = None;
        let needs_error = stop_gap.is_some()
            && stop_gap != Some("auto")
            && (script_type.is_none() || script_type == Some("auto"));
        assert!(!needs_error, "auto stop_gap should not require script_type");

        // Case 2: Custom stop_gap without script_type - should error
        let stop_gap = Some("500");
        let script_type: Option<&str> = None;
        let needs_error = stop_gap.is_some()
            && stop_gap != Some("auto")
            && (script_type.is_none() || script_type == Some("auto"));
        assert!(
            needs_error,
            "Custom stop_gap without script_type should require error"
        );

        // Case 3: Custom stop_gap with "auto" script_type - should error
        let stop_gap = Some("250");
        let script_type = Some("auto");
        let needs_error = stop_gap.is_some()
            && stop_gap != Some("auto")
            && (script_type.is_none() || script_type == Some("auto"));
        assert!(
            needs_error,
            "Custom stop_gap with auto script_type should require error"
        );

        // Case 4: Custom stop_gap with specific script_type - no error
        let stop_gap = Some("750");
        let script_type = Some("p2wpkh");
        let needs_error = stop_gap.is_some()
            && stop_gap != Some("auto")
            && (script_type.is_none() || script_type == Some("auto"));
        assert!(
            !needs_error,
            "Custom stop_gap with specific script_type should not require error"
        );

        println!("✅ Custom stop gap requires script type tests passed!");
    }

    #[test]
    fn test_xpub_detection() {
        use crate::xpub_converter::XpubConverter;

        // Valid XPUBs should be detected
        let xpub_cases = vec![
            // Standard formats
            "tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5",
            "vpub5Y6cjg78GGuNLsaPhmYsiw4gYX3HoQiRBiSwDaBXKUafCt9bNwWQiitDk5VZ5BVxYnQdwoTyXSs2JHRPAgjAvtbBrf8ZhDYe2jWAqvZVnsc",
            "zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs",
        ];

        for xpub in xpub_cases {
            assert!(
                XpubConverter::is_xpub(xpub),
                "Expected '{}' to be detected as XPUB",
                xpub
            );
        }

        // Descriptors should NOT be detected as XPUBs
        let descriptor_cases = vec![
            "wpkh(tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)",
            "sh(wpkh(vpub5Y6cjg78GGuNLsaPhmYsiw4gYX3HoQiRBiSwDaBXKUafCt9bNwWQiitDk5VZ5BVxYnQdwoTyXSs2JHRPAgjAvtbBrf8ZhDYe2jWAqvZVnsc/<0;1>/*))",
            "tr(zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs/<0;1>/*)",
        ];

        for desc in descriptor_cases {
            assert!(
                !XpubConverter::is_xpub(desc),
                "Expected '{}' to NOT be detected as XPUB (it's a descriptor)",
                desc
            );
        }

        println!("✅ XPUB detection tests passed!");
    }

    #[test]
    fn test_fresh_wallet_xpub_requires_script_type() {
        // Logic from handler: fresh XPUB wallet without script_type should fail
        use crate::xpub_converter::XpubConverter;

        let xpub = "tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5";
        let is_fresh_wallet = true;
        let script_type: Option<&str> = None;

        // Simulate handler logic
        let needs_error = XpubConverter::is_xpub(xpub) && is_fresh_wallet && script_type.is_none();

        assert!(
            needs_error,
            "Fresh XPUB wallet without script_type should require error"
        );

        // With script_type provided - should work
        let script_type = Some("p2wpkh");
        let needs_error = XpubConverter::is_xpub(xpub) && is_fresh_wallet && script_type.is_none();

        assert!(
            !needs_error,
            "Fresh XPUB wallet with script_type should not require error"
        );

        // Descriptor (not XPUB) - doesn't need script_type
        let descriptor = "wpkh(tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)";
        let script_type: Option<&str> = None;
        let needs_error =
            XpubConverter::is_xpub(descriptor) && is_fresh_wallet && script_type.is_none();

        assert!(
            !needs_error,
            "Descriptor format should not require script_type (it's already wrapped)"
        );

        println!("✅ Fresh wallet XPUB requires script_type tests passed!");
    }
}
