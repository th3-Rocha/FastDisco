//! The saved token, in the OS keyring. Blocking calls: run them on the
//! backend, never on the interface thread.

use std::sync::Arc;

use anyhow::{Context, Result};
use keyring_core::api::CredentialStoreApi;

const SERVICE: &str = "fastdisco";
const ACCOUNT: &str = "token";

pub fn load() -> Result<Option<String>> {
    match entry()?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(error) => Err(error).context("Could not read the token from the OS keyring"),
    }
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
        store.context("Unlock your OS keyring and restart FastDisco")?
            as Arc<dyn CredentialStoreApi>,
    )
}
