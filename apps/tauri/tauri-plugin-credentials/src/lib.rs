use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};
#[cfg(target_os = "android")]
use tauri::plugin::PluginHandle;

const PLUGIN_IDENTIFIER: &str = "app.tauri.credentials";

#[derive(Serialize, Deserialize)]
struct StoreArgs {
    domain: String,
    username: String,
    password: String,
}

#[derive(Serialize, Deserialize)]
struct DomainArgs {
    domain: String,
}

#[derive(Serialize, Deserialize)]
struct LoadResult {
    username: String,
    password: String,
}

/// Credential storage handle. Desktop uses the system keychain; Android uses EncryptedSharedPreferences.
pub struct Credentials<R: Runtime> {
    #[cfg(target_os = "android")]
    _handle: PluginHandle<R>,
    #[cfg(not(target_os = "android"))]
    _phantom: std::marker::PhantomData<fn() -> R>,
}

impl<R: Runtime> Credentials<R> {
    pub fn store(&self, domain: &str, username: &str, password: &str) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            self._handle
                .run_mobile_plugin::<()>(
                    "store",
                    StoreArgs {
                        domain: domain.to_string(),
                        username: username.to_string(),
                        password: password.to_string(),
                    },
                )
                .map_err(|e| e.to_string())
        }
        #[cfg(not(target_os = "android"))]
        {
            desktop_store(domain, username, password)
        }
    }

    pub fn load(&self, domain: &str) -> Result<(String, String), String> {
        #[cfg(target_os = "android")]
        {
            let result: LoadResult = self
                ._handle
                .run_mobile_plugin("load", DomainArgs { domain: domain.to_string() })
                .map_err(|e| e.to_string())?;
            Ok((result.username, result.password))
        }
        #[cfg(not(target_os = "android"))]
        {
            desktop_load(domain)
        }
    }

    pub fn delete(&self, domain: &str) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            self._handle
                .run_mobile_plugin::<()>("delete", DomainArgs { domain: domain.to_string() })
                .map_err(|e| e.to_string())
        }
        #[cfg(not(target_os = "android"))]
        {
            desktop_delete(domain)
        }
    }
}

// ── Desktop keyring implementation ──────────────────────────────────

#[cfg(all(not(target_os = "android"), feature = "desktop"))]
fn desktop_store(domain: &str, username: &str, password: &str) -> Result<(), String> {
    let service = format!("com.onyx.webdav.{}", domain);
    let scoped_service = format!("com.onyx.webdav.{}::{}", domain, username);

    keyring::Entry::new(&service, "username")
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?
        .set_password(username)
        .map_err(|e| format!("Failed to store username: {}", e))?;

    keyring::Entry::new(&scoped_service, "password")
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?
        .set_password(password)
        .map_err(|e| format!("Failed to store password: {}", e))?;

    if let Ok(legacy) = keyring::Entry::new(&service, "password") {
        let _ = legacy.delete_credential();
    }
    Ok(())
}

#[cfg(all(not(target_os = "android"), not(feature = "desktop")))]
fn desktop_store(_domain: &str, _username: &str, _password: &str) -> Result<(), String> {
    Err("Credential storage not available on this platform".into())
}

#[cfg(all(not(target_os = "android"), feature = "desktop"))]
fn desktop_load(domain: &str) -> Result<(String, String), String> {
    let service = format!("com.onyx.webdav.{}", domain);

    let username = keyring::Entry::new(&service, "username")
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?
        .get_password()
        .map_err(|_| {
            format!(
                "No credentials found for '{}'. Run setup or configure environment variables.",
                domain
            )
        })?;

    let scoped_service = format!("com.onyx.webdav.{}::{}", domain, username);
    let password = keyring::Entry::new(&scoped_service, "password")
        .ok()
        .and_then(|e| e.get_password().ok())
        .or_else(|| {
            keyring::Entry::new(&service, "password")
                .ok()
                .and_then(|e| e.get_password().ok())
        })
        .ok_or_else(|| format!("No password found for '{}' user '{}'", domain, username))?;

    // Auto-migrate legacy credentials to scoped format
    if keyring::Entry::new(&scoped_service, "password")
        .ok()
        .and_then(|e| e.get_password().ok())
        .is_none()
    {
        if let Ok(entry) = keyring::Entry::new(&scoped_service, "password") {
            let _ = entry.set_password(&password);
        }
        if let Ok(legacy) = keyring::Entry::new(&service, "password") {
            let _ = legacy.delete_credential();
        }
    }

    Ok((username, password))
}

#[cfg(all(not(target_os = "android"), not(feature = "desktop")))]
fn desktop_load(domain: &str) -> Result<(String, String), String> {
    Err(format!(
        "No credentials found for '{}'. Credential storage not available on this platform.",
        domain
    ))
}

#[cfg(all(not(target_os = "android"), feature = "desktop"))]
fn desktop_delete(domain: &str) -> Result<(), String> {
    let service = format!("com.onyx.webdav.{}", domain);
    let username = keyring::Entry::new(&service, "username")
        .ok()
        .and_then(|e| e.get_password().ok());

    if let Some(user) = &username {
        let scoped = format!("com.onyx.webdav.{}::{}", domain, user);
        if let Ok(e) = keyring::Entry::new(&scoped, "password") {
            let _ = e.delete_credential();
        }
    }
    if let Ok(e) = keyring::Entry::new(&service, "password") {
        let _ = e.delete_credential();
    }
    if let Ok(e) = keyring::Entry::new(&service, "username") {
        let _ = e.delete_credential();
    }
    Ok(())
}

#[cfg(all(not(target_os = "android"), not(feature = "desktop")))]
fn desktop_delete(_domain: &str) -> Result<(), String> {
    Ok(())
}

// ── Plugin init ─────────────────────────────────────────────────────

/// Initialize the credentials plugin. Call `.plugin(tauri_plugin_credentials::init())` on the Tauri builder.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("credentials")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            let credentials = Credentials {
                _handle: api.register_android_plugin(PLUGIN_IDENTIFIER, "CredentialPlugin")?,
            };
            #[cfg(not(target_os = "android"))]
            let credentials: Credentials<R> = Credentials {
                _phantom: std::marker::PhantomData,
            };
            let _ = api;
            app.manage(credentials);
            Ok(())
        })
        .build()
}
