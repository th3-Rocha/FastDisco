//! The saved token, in the OS keyring. Blocking calls: run them on the
//! backend, never on the interface thread.

use std::sync::Arc;

use anyhow::{Context, Result};
use keyring_core::api::CredentialStoreApi;

const SERVICE: &str = "discofast";
const ACCOUNT: &str = "token";

pub fn load() -> Result<Option<String>> {
    match entry()?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring_core::Error::NoEntry) => load_legacy(),
        Err(error) => Err(error).context("Could not read the token from the OS keyring"),
    }
}

/// The token FastDisco saved before the rename, moved under the new name
/// so the user stays signed in.
fn load_legacy() -> Result<Option<String>> {
    let legacy = store()?
        .build(crate::updates::LEGACY_SLUG, ACCOUNT, None)
        .context("Could not open the OS keyring")?;
    let Ok(token) = legacy.get_password() else {
        return Ok(None);
    };
    save(&token)?;
    if let Err(error) = legacy.delete_credential() {
        log::warn!("keyring: could not remove FastDisco's token: {error}");
    }
    Ok(Some(token))
}

pub fn save(token: &str) -> Result<()> {
    entry()?
        .set_password(token)
        .context("Could not save the token in the OS keyring")
}

pub fn forget() -> Result<()> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(error) => Err(error).context("Could not remove the token from the OS keyring"),
    }
}

fn entry() -> Result<keyring_core::Entry> {
    store()?
        .build(SERVICE, ACCOUNT, None)
        .context("Could not open the OS keyring")
}

fn store() -> Result<Arc<dyn CredentialStoreApi>> {
    #[cfg(target_os = "linux")]
    let store = zbus_secret_service_keyring_store::Store::new();
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new();
    #[cfg(windows)]
    let store = windows_native_keyring_store::Store::new();
    Ok(
        store.context("Unlock your OS keyring and restart Discofast")?
            as Arc<dyn CredentialStoreApi>,
    )
}
