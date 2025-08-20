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
            assert!(result.starts_with(expected_prefix), 
                "Expected '{}' to start with '{}' ({})", 
                result, expected_prefix, description);
            
            // Check that the result has a checksum (ends with #xxxxxxxx)
            assert!(result.contains('#'), 
                "Expected result to contain checksum (#): {} ({})", 
                result, description);
            
            // Check that key origin was actually stripped
            assert!(!result.contains('['), 
                "Expected key origin to be stripped from: {} ({})", 
                result, description);
        }

        // Test that the same XPUB with different fingerprints produces the same checksum
        let input1 = "wpkh([805c684b/84h/1h/0h]tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)#8nt3y08q";
        let input2 = "wpkh([12345678/84h/1h/0h]tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)#different";
        
        let result1 = test_strip_key_origin(input1).unwrap();
        let result2 = test_strip_key_origin(input2).unwrap();
        
        println!("Same XPUB test:");
        println!("Result1: {}", result1);
        println!("Result2: {}", result2);
        
        assert_eq!(result1, result2, 
            "Same XPUB with different fingerprints should produce same normalized descriptor");

        println!("✅ All tests passed!");
    }

    #[test]
    fn test_xpub_normalization() {
        use crate::xpub_converter::XpubConverter;
        use bdk_wallet::bitcoin::Network;

        // Test normalization for different networks and extended key formats
        let test_cases = vec![
            // Mainnet cases
            (Network::Bitcoin, "xpub6BmTxpDFqy", "xpub6BmTxpDFqy"), // Already normalized
            (Network::Bitcoin, "ypub6Ww3ibxVfGzLrAH1PNcjyAWenMTbbAosGNpj7MmV", "xpub6Ww3ibxVfGzLrAH1PNcjyAWenMTbbAosGNpj7MmV"),
            (Network::Bitcoin, "zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs", "xpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs"),
            
            // Testnet cases
            (Network::Testnet, "tpub6BmTxpDFqy", "tpub6BmTxpDFqy"), // Already normalized
            (Network::Testnet, "upub5EFU65HtV5TeiSHmZZm7FUffBGy8UKeqp7vw43jYbvjNECs", "tpub5EFU65HtV5TeiSHmZZm7FUffBGy8UKeqp7vw43jYbvjNECs"),
            (Network::Testnet, "vpub5Y6cjg78GGuNLsaPhmYsiw4gYX3HoQiRBiSwDaBXKUafCt9bNwWQiitDk5VZ5BVxYnQdwoTyXSs2JHRPAgjAvtbBrf8ZhDYe2jWAqvZVnsc", "tpub5Y6cjg78GGuNLsaPhmYsiw4gYX3HoQiRBiSwDaBXKUafCt9bNwWQiitDk5VZ5BVxYnQdwoTyXSs2JHRPAgjAvtbBrf8ZhDYe2jWAqvZVnsc"),
            
            // Cross-network normalization (mainnet keys on testnet)
            (Network::Testnet, "xpub6BmTxpDFqy", "tpub6BmTxpDFqy"),
            (Network::Testnet, "ypub6Ww3ibxVfGzL", "tpub6Ww3ibxVfGzL"),
            (Network::Testnet, "zpub6rFR7y4Q2Aij", "tpub6rFR7y4Q2Aij"),
            
            // Regtest (uses testnet format)
            (Network::Regtest, "xpub6BmTxpDFqy", "tpub6BmTxpDFqy"),
            (Network::Regtest, "tpub6BmTxpDFqy", "tpub6BmTxpDFqy"),
        ];

        for (network, input, expected) in test_cases {
            let converter = XpubConverter::new(network, None);
            let result = converter.normalize_xpub(input).unwrap();
            assert_eq!(result, expected, 
                "Network: {:?}, Input: {} -> Expected: {}, Got: {}", 
                network, input, expected, result);
        }

        println!("✅ XPUB normalization tests passed!");
    }
}