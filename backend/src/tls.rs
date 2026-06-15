pub fn install_default_rustls_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return;
    }

    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[cfg(test)]
mod tests {
    use super::install_default_rustls_crypto_provider;

    #[test]
    fn rustls_crypto_provider_install_is_idempotent() {
        install_default_rustls_crypto_provider();
        install_default_rustls_crypto_provider();

        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }
}
