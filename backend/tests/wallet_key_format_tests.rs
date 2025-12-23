use bdk_wallet::bitcoin::Network;
use canary::{
    config::{AppConfig, NetworkConfig, OperatingMode},
    metadata::MetadataDb,
    xpub_converter::XpubConverter,
};
use std::sync::Arc;
use tempfile::TempDir;

/// Test that zpub and xpub formats normalize to the same value
/// This ensures that adding a wallet with zpub and then trying to add
/// the same wallet with xpub (or vice versa) will be detected as duplicate
#[test]
fn test_zpub_xpub_normalize_to_same_value() {
    // Bacon wallet keys - these are the same key in different formats
    let bacon_xpub = "xpub6DEzNop46vmxR49zYWFnMwmEfawSNmAMf6dLH5YKDY463twtvw1XD7ihwJRLPRGZJz799VPFzXHpZu6WdhT29WnaeuChS6aZHZPFmqczR5K";
    let bacon_zpub = "zpub6ruWz99tQHrv7eYEDDq2n7xF1XELG19MVKfmqsL5yYorA6aMSFLeTF2yyiLWPEaQ8GLkeSaNuqzvLUKe56H3jz9nPabYbvDXq1WYZ5NMmEk";

    let converter = XpubConverter::new(Network::Bitcoin, None);

    let normalized_xpub = converter.normalize_xpub(bacon_xpub).unwrap();
    let normalized_zpub = converter.normalize_xpub(bacon_zpub).unwrap();

    assert_eq!(
        normalized_xpub, normalized_zpub,
        "zpub and xpub of the same key should normalize to the same xpub value"
    );

    // Both should be the xpub format
    assert!(
        normalized_xpub.starts_with("xpub"),
        "Normalized key should be in xpub format"
    );
    assert!(
        normalized_zpub.starts_with("xpub"),
        "Normalized zpub should convert to xpub format"
    );

    println!(
        "✅ zpub and xpub normalize to same value: {}",
        normalized_xpub
    );
}

/// Test that vpub and tpub formats normalize to the same value on testnet
#[test]
fn test_vpub_tpub_normalize_to_same_value() {
    // Bacon wallet testnet keys - same key in different formats
    let bacon_tpub = "tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi";
    let bacon_vpub = "vpub5YKG2ySm6NEAiVBfzKXV2GqAW2UmhhrtWvPs95k86Rxoa1BaW6acVJmbbNa7pvMQwpEppvuMezYdY2ujN1UF6gj7eoZWYwJLNPrdhwjjPTN";

    let converter = XpubConverter::new(Network::Testnet, None);

    let normalized_tpub = converter.normalize_xpub(bacon_tpub).unwrap();
    let normalized_vpub = converter.normalize_xpub(bacon_vpub).unwrap();

    assert_eq!(
        normalized_tpub, normalized_vpub,
        "vpub and tpub of the same key should normalize to the same tpub value"
    );

    // Both should be the tpub format
    assert!(
        normalized_tpub.starts_with("tpub"),
        "Normalized key should be in tpub format"
    );
    assert!(
        normalized_vpub.starts_with("tpub"),
        "Normalized vpub should convert to tpub format"
    );

    println!(
        "✅ vpub and tpub normalize to same value: {}",
        normalized_tpub
    );
}

/// Test that vpub and tpub formats normalize to the same value on regtest
#[test]
fn test_vpub_tpub_normalize_to_same_value_regtest() {
    let bacon_tpub = "tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi";
    let bacon_vpub = "vpub5YKG2ySm6NEAiVBfzKXV2GqAW2UmhhrtWvPs95k86Rxoa1BaW6acVJmbbNa7pvMQwpEppvuMezYdY2ujN1UF6gj7eoZWYwJLNPrdhwjjPTN";

    let converter = XpubConverter::new(Network::Regtest, None);

    let normalized_tpub = converter.normalize_xpub(bacon_tpub).unwrap();
    let normalized_vpub = converter.normalize_xpub(bacon_vpub).unwrap();

    assert_eq!(
        normalized_tpub, normalized_vpub,
        "vpub and tpub of the same key should normalize to the same tpub value on regtest"
    );

    println!(
        "✅ vpub and tpub normalize to same value on regtest: {}",
        normalized_tpub
    );
}

// TODO: Add tests for ypub (mainnet BIP49) and upub (testnet BIP49) formats
// when valid test keys become available. These are nested segwit formats that
// should also normalize correctly:
// - ypub -> xpub (mainnet)
// - upub -> tpub (testnet/regtest)

/// Test that descriptors generated from zpub and xpub produce the same checksum
#[test]
fn test_descriptors_from_zpub_xpub_have_same_checksum() {
    use canary::xpub_converter::ScriptType;

    let bacon_xpub = "xpub6DEzNop46vmxR49zYWFnMwmEfawSNmAMf6dLH5YKDY463twtvw1XD7ihwJRLPRGZJz799VPFzXHpZu6WdhT29WnaeuChS6aZHZPFmqczR5K";
    let bacon_zpub = "zpub6ruWz99tQHrv7eYEDDq2n7xF1XELG19MVKfmqsL5yYorA6aMSFLeTF2yyiLWPEaQ8GLkeSaNuqzvLUKe56H3jz9nPabYbvDXq1WYZ5NMmEk";

    let converter = XpubConverter::new(Network::Bitcoin, None);

    // Generate Native SegWit (P2WPKH) descriptors from both formats
    let descriptor_from_xpub = converter
        .generate_descriptor_for_type(bacon_xpub, &ScriptType::P2WPKH)
        .unwrap();
    let descriptor_from_zpub = converter
        .generate_descriptor_for_type(bacon_zpub, &ScriptType::P2WPKH)
        .unwrap();

    assert_eq!(
        descriptor_from_xpub, descriptor_from_zpub,
        "Descriptors generated from zpub and xpub should be identical"
    );

    // Extract checksums
    let checksum_from_xpub = descriptor_from_xpub.split('#').last().unwrap();
    let checksum_from_zpub = descriptor_from_zpub.split('#').last().unwrap();

    assert_eq!(
        checksum_from_xpub, checksum_from_zpub,
        "Checksums should be identical"
    );

    println!("✅ Descriptors from zpub and xpub are identical:");
    println!("   From xpub: {}", descriptor_from_xpub);
    println!("   From zpub: {}", descriptor_from_zpub);
}

/// Test that descriptors generated from vpub and tpub produce the same checksum
#[test]
fn test_descriptors_from_vpub_tpub_have_same_checksum() {
    use canary::xpub_converter::ScriptType;

    let bacon_tpub = "tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi";
    let bacon_vpub = "vpub5YKG2ySm6NEAiVBfzKXV2GqAW2UmhhrtWvPs95k86Rxoa1BaW6acVJmbbNa7pvMQwpEppvuMezYdY2ujN1UF6gj7eoZWYwJLNPrdhwjjPTN";

    let converter = XpubConverter::new(Network::Testnet, None);

    // Generate Native SegWit (P2WPKH) descriptors from both formats
    let descriptor_from_tpub = converter
        .generate_descriptor_for_type(bacon_tpub, &ScriptType::P2WPKH)
        .unwrap();
    let descriptor_from_vpub = converter
        .generate_descriptor_for_type(bacon_vpub, &ScriptType::P2WPKH)
        .unwrap();

    assert_eq!(
        descriptor_from_tpub, descriptor_from_vpub,
        "Descriptors generated from vpub and tpub should be identical"
    );

    println!("✅ Descriptors from vpub and tpub are identical:");
    println!("   From tpub: {}", descriptor_from_tpub);
    println!("   From vpub: {}", descriptor_from_vpub);
}

/// Integration test: Verify that adding a wallet with one format prevents
/// adding the same wallet with a different format (database level)
#[tokio::test]
async fn test_duplicate_wallet_detection_zpub_xpub() {
    use canary::xpub_converter::ScriptType;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let config = AppConfig {
        network: NetworkConfig::Mainnet,
        electrum_url: Some("ssl://electrum.blockstream.info:50002".to_string()),
        bind_address: "127.0.0.1:3000".to_string(),
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        operating_mode: OperatingMode::SelfHosted,
    };

    let metadata_db = Arc::new(
        MetadataDb::new(db_path.to_str().unwrap(), &config)
            .await
            .unwrap(),
    );

    // Create a test user
    let user_id = metadata_db
        .create_user(
            "test@example.com",
            "hash",
            Some("Test User"),
            false,
            None,
            None,
        )
        .await
        .unwrap();

    let bacon_xpub = "xpub6DEzNop46vmxR49zYWFnMwmEfawSNmAMf6dLH5YKDY463twtvw1XD7ihwJRLPRGZJz799VPFzXHpZu6WdhT29WnaeuChS6aZHZPFmqczR5K";
    let bacon_zpub = "zpub6ruWz99tQHrv7eYEDDq2n7xF1XELG19MVKfmqsL5yYorA6aMSFLeTF2yyiLWPEaQ8GLkeSaNuqzvLUKe56H3jz9nPabYbvDXq1WYZ5NMmEk";

    let converter = XpubConverter::new(Network::Bitcoin, None);

    // Generate descriptors from both formats
    let descriptor_from_xpub = converter
        .generate_descriptor_for_type(bacon_xpub, &ScriptType::P2WPKH)
        .unwrap();
    let descriptor_from_zpub = converter
        .generate_descriptor_for_type(bacon_zpub, &ScriptType::P2WPKH)
        .unwrap();

    // Add wallet using xpub-derived descriptor
    let wallet_checksum = metadata_db
        .insert_wallet("Bacon (xpub)", &descriptor_from_xpub, &user_id)
        .await
        .unwrap();

    println!("✅ Created wallet with checksum: {}", wallet_checksum);

    // Verify the descriptor exists
    let exists_xpub = metadata_db
        .descriptor_exists(&descriptor_from_xpub)
        .await
        .unwrap();
    assert!(exists_xpub, "Descriptor from xpub should exist");

    // The zpub-derived descriptor should be identical, so it should also "exist"
    let exists_zpub = metadata_db
        .descriptor_exists(&descriptor_from_zpub)
        .await
        .unwrap();
    assert!(
        exists_zpub,
        "Descriptor from zpub should also be detected as existing (same wallet)"
    );

    // Trying to insert the zpub-derived descriptor should fail with UNIQUE constraint
    let result = metadata_db
        .insert_wallet("Bacon (zpub)", &descriptor_from_zpub, &user_id)
        .await;

    assert!(
        result.is_err(),
        "Should not be able to add same wallet with zpub format"
    );

    println!("✅ Duplicate wallet detection works - zpub wallet was rejected");
}

/// Integration test: Verify duplicate detection with vpub/tpub on testnet
#[tokio::test]
async fn test_duplicate_wallet_detection_vpub_tpub() {
    use canary::xpub_converter::ScriptType;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let config = AppConfig {
        network: NetworkConfig::Testnet,
        electrum_url: Some("ssl://electrum.blockstream.info:60002".to_string()),
        bind_address: "127.0.0.1:3000".to_string(),
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        operating_mode: OperatingMode::SelfHosted,
    };

    let metadata_db = Arc::new(
        MetadataDb::new(db_path.to_str().unwrap(), &config)
            .await
            .unwrap(),
    );

    // Create a test user
    let user_id = metadata_db
        .create_user(
            "test@example.com",
            "hash",
            Some("Test User"),
            false,
            None,
            None,
        )
        .await
        .unwrap();

    let bacon_tpub = "tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi";
    let bacon_vpub = "vpub5YKG2ySm6NEAiVBfzKXV2GqAW2UmhhrtWvPs95k86Rxoa1BaW6acVJmbbNa7pvMQwpEppvuMezYdY2ujN1UF6gj7eoZWYwJLNPrdhwjjPTN";

    let converter = XpubConverter::new(Network::Testnet, None);

    // Generate descriptors from both formats
    let descriptor_from_tpub = converter
        .generate_descriptor_for_type(bacon_tpub, &ScriptType::P2WPKH)
        .unwrap();
    let descriptor_from_vpub = converter
        .generate_descriptor_for_type(bacon_vpub, &ScriptType::P2WPKH)
        .unwrap();

    // Add wallet using tpub-derived descriptor
    let wallet_checksum = metadata_db
        .insert_wallet("Bacon (tpub)", &descriptor_from_tpub, &user_id)
        .await
        .unwrap();

    println!("✅ Created wallet with checksum: {}", wallet_checksum);

    // Trying to insert the vpub-derived descriptor should fail
    let result = metadata_db
        .insert_wallet("Bacon (vpub)", &descriptor_from_vpub, &user_id)
        .await;

    assert!(
        result.is_err(),
        "Should not be able to add same wallet with vpub format"
    );

    println!("✅ Duplicate wallet detection works - vpub wallet was rejected");
}
