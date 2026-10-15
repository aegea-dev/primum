//! Secrets: the values `${env.NAME}` stands for. They are kept in the OS
//! secret store (macOS Keychain, Windows Credential Manager, the Secret
//! Service on Linux) under the service `primum`, and looked up there first,
//! then in the daemon's own environment. primum never writes one to a file.
use anyhow::{Result, anyhow, bail};

const SERVICE: &str = "primum";
/// The names of the secrets primum keeps: stores cannot list their entries.
const INDEX: &str = "primum.names";

/// A secret's name is an environment variable's: a letter or `_`, then
/// letters, digits or `_`.
pub fn valid_name(name: &str) -> bool {
    let mut cs = name.chars();
    cs.next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `PRIMUM_NO_SECRET_STORE` keeps primum away from the OS secret store.
pub fn store_off() -> bool {
    std::env::var_os("PRIMUM_NO_SECRET_STORE").is_some()
}

fn entry(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, name).map_err(|e| anyhow!("the OS secret store: {e}"))
}

/// Whether a secret can be kept or removed: a valid name, and the store on.
pub fn ready(name: &str) -> Result<()> {
    if !valid_name(name) {
        bail!("{name}: a secret's name is a letter or _, then letters, digits or _");
    }
    if store_off() {
        bail!("the OS secret store is off (PRIMUM_NO_SECRET_STORE is set)");
    }
    Ok(())
}

/// A secret's value: from the store, else from the environment. A store
/// that cannot be reached counts as having nothing.
pub fn get(name: &str) -> Option<String> {
    if !store_off()
        && let Ok(e) = keyring::Entry::new(SERVICE, name)
        && let Ok(v) = e.get_password()
    {
        return Some(v);
    }
    std::env::var(name).ok()
}

/// The names of the secrets in the store, sorted.
pub fn names() -> Result<Vec<String>> {
    if store_off() {
        bail!("the OS secret store is off (PRIMUM_NO_SECRET_STORE is set)");
    }
    match entry(INDEX)?.get_password() {
        Ok(text) => Ok(serde_json::from_str(&text).unwrap_or_default()),
        Err(keyring::Error::NoEntry) => Ok(vec![]),
        Err(e) => Err(anyhow!("the OS secret store: {e}")),
    }
}

fn save_names(mut names: Vec<String>) -> Result<()> {
    names.sort();
    names.dedup();
    entry(INDEX)?
        .set_password(&serde_json::to_string(&names)?)
        .map_err(|e| anyhow!("the OS secret store: {e}"))
}

/// Keeps a secret in the store.
pub fn set(name: &str, value: &str) -> Result<()> {
    ready(name)?;
    entry(name)?
        .set_password(value)
        .map_err(|e| anyhow!("the OS secret store: {e}"))?;
    let mut all = names()?;
    all.push(name.to_string());
    save_names(all)
}

/// Removes a secret from the store; false when it was not there.
pub fn unset(name: &str) -> Result<bool> {
    ready(name)?;
    let had = match entry(name)?.delete_credential() {
        Ok(()) => true,
        Err(keyring::Error::NoEntry) => false,
        Err(e) => return Err(anyhow!("the OS secret store: {e}")),
    };
    let all: Vec<String> = names()?.into_iter().filter(|n| n != name).collect();
    save_names(all)?;
    Ok(had)
}
