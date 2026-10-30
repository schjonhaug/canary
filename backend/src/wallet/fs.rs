//! Owner-only filesystem permissions for the wallet directory and the BDK
//! `SQLite` files inside it.

use std::path::PathBuf;

pub(super) fn restrict_permissions(path: &std::path::Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, PermissionsExt::from_mode(mode))
}

pub(super) fn create_private_dir(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true).mode(0o700).create(path)?;
    restrict_permissions(path, 0o700)
}

pub(super) fn create_private_file(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    Ok(())
}

pub(super) fn sqlite_sidecar_path(path: &std::path::Path, suffix: &str) -> PathBuf {
    let mut sidecar = path.as_os_str().to_os_string();
    sidecar.push(suffix);
    PathBuf::from(sidecar)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restrict_permissions_sets_owner_only_modes() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempfile::tempdir().unwrap();
        let directory_path = temp_dir.path().join("wallets");
        let file_path = directory_path.join("wallet.sqlite");

        create_private_dir(&directory_path).unwrap();
        create_private_file(&file_path).unwrap();

        assert_eq!(
            std::fs::metadata(&directory_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&file_path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        assert!(restrict_permissions(&temp_dir.path().join("missing"), 0o600).is_err());
    }
}
