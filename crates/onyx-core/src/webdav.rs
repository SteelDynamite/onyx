use reqwest::Client;
use zeroize::Zeroizing;
use std::time::Duration;
use crate::error::{Error, Result};

/// Hard timeout for any WebDAV network operation.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Maximum size for file downloads (10 MB).
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// Information about a file on the remote WebDAV server.
#[derive(Debug, Clone)]
pub struct RemoteFileInfo {
    pub path: String,
    pub is_dir: bool,
    pub content_length: u64,
    pub last_modified: Option<String>,
}

/// WebDAV client wrapping reqwest with basic auth. Credentials are zeroized on drop.
pub struct WebDavClient {
    client: Client,
    base_url: String,
    username: Zeroizing<String>,
    password: Zeroizing<String>,
}

impl WebDavClient {
    /// Create a new WebDAV client. Rejects non-HTTPS URLs to prevent sending credentials in plaintext.
    pub fn new(base_url: &str, username: &str, password: &str) -> Result<Self> {
        if !base_url.starts_with("https://") {
            return Err(Error::WebDav("Refusing non-HTTPS URL: credentials would be sent in plaintext".into()));
        }
        Self::new_unchecked(base_url, username, password)
    }

    fn new_unchecked(base_url: &str, username: &str, password: &str) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|e| Error::WebDav(format!("Failed to build HTTP client: {}", e)))?;
        Ok(Self {
            client,
            base_url,
            username: Zeroizing::new(username.to_string()),
            password: Zeroizing::new(password.to_string()),
        })
    }

    fn full_url(&self, path: &str) -> String {
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            self.base_url.clone()
        } else {
            // Percent-encode path segments while preserving '/'
            let encoded: String = path
                .split('/')
                .map(percent_encode)
                .collect::<Vec<_>>()
                .join("/");
            format!("{}/{}", self.base_url, encoded)
        }
    }

    /// Test connection by issuing a PROPFIND depth 0 on the root.
    pub async fn test_connection(&self) -> Result<()> {
        let resp = self.client
            .request(reqwest::Method::from_bytes(b"PROPFIND").expect("PROPFIND is a valid HTTP method"), &self.base_url)
            .basic_auth(self.username.as_str(), Some(self.password.as_str()))
            .header("Depth", "0")
            .header("Content-Type", "application/xml")
            .body(PROPFIND_BODY)
            .send()
            .await?;

        let status = resp.status().as_u16();
        if status == 207 || status == 200 {
            Ok(())
        } else if status == 401 || status == 403 {
            Err(Error::Credential("Authentication failed".to_string()))
        } else {
            Err(Error::WebDav(format!("Unexpected status {}", status)))
        }
    }

    /// List files at a given path using PROPFIND depth 1.
    pub async fn list_files(&self, path: &str) -> Result<Vec<RemoteFileInfo>> {
        let url = self.full_url(path);
        let resp = self.client
            .request(reqwest::Method::from_bytes(b"PROPFIND").expect("PROPFIND is a valid HTTP method"), &url)
            .basic_auth(self.username.as_str(), Some(self.password.as_str()))
            .header("Depth", "1")
            .header("Content-Type", "application/xml")
            .body(PROPFIND_BODY)
            .send()
            .await?;

        let status = resp.status().as_u16();
        if status != 207 {
            return Err(Error::WebDav(format!("PROPFIND failed with status {}", status)));
        }

        // Reject oversized responses to prevent memory exhaustion from malicious servers
        const MAX_PROPFIND_BYTES: u64 = 10 * 1024 * 1024;
        if resp.content_length().unwrap_or(0) > MAX_PROPFIND_BYTES {
            return Err(Error::WebDav("PROPFIND response too large (>10MB)".into()));
        }
        let bytes = resp.bytes().await?;
        if bytes.len() as u64 > MAX_PROPFIND_BYTES {
            return Err(Error::WebDav("PROPFIND response too large (>10MB)".into()));
        }
        let body = String::from_utf8_lossy(&bytes);
        parse_propfind_response(&body, &self.base_url, path)
    }

    /// Download a file's contents.
    pub async fn get_file(&self, path: &str) -> Result<Vec<u8>> {
        let url = self.full_url(path);
        let resp = self.client
            .get(&url)
            .basic_auth(self.username.as_str(), Some(self.password.as_str()))
            .send()
            .await?;

        let status = resp.status().as_u16();
        if status == 404 {
            return Err(Error::NotFound(format!("Remote file not found: {}", path)));
        }
        if status != 200 {
            return Err(Error::WebDav(format!("GET failed with status {}", status)));
        }

        if resp.content_length().unwrap_or(0) > MAX_FILE_BYTES {
            return Err(Error::WebDav(format!("File too large (>{}MB)", MAX_FILE_BYTES / (1024 * 1024))));
        }
        let bytes = resp.bytes().await?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(Error::WebDav(format!("File too large (>{}MB)", MAX_FILE_BYTES / (1024 * 1024))));
        }

        Ok(bytes.to_vec())
    }

    /// Upload a file.
    pub async fn put_file(&self, path: &str, content: Vec<u8>) -> Result<()> {
        let url = self.full_url(path);
        let resp = self.client
            .put(&url)
            .basic_auth(self.username.as_str(), Some(self.password.as_str()))
            .body(content)
            .send()
            .await?;

        let status = resp.status().as_u16();
        if !(200..=299).contains(&status) {
            return Err(Error::WebDav(format!("PUT failed with status {}", status)));
        }
        Ok(())
    }

    /// Delete a remote file.
    pub async fn delete_file(&self, path: &str) -> Result<()> {
        let url = self.full_url(path);
        let resp = self.client
            .delete(&url)
            .basic_auth(self.username.as_str(), Some(self.password.as_str()))
            .send()
            .await?;

        let status = resp.status().as_u16();
        if status == 404 {
            return Ok(()); // Already gone
        }
        if !(200..=299).contains(&status) {
            return Err(Error::WebDav(format!("DELETE failed with status {}", status)));
        }
        Ok(())
    }

    /// Create a directory via MKCOL.
    pub async fn create_dir(&self, path: &str) -> Result<()> {
        let url = self.full_url(path);
        let resp = self.client
            .request(reqwest::Method::from_bytes(b"MKCOL").expect("MKCOL is a valid HTTP method"), &url)
            .basic_auth(self.username.as_str(), Some(self.password.as_str()))
            .send()
            .await?;

        let status = resp.status().as_u16();
        if status == 405 {
            return Ok(()); // Already exists
        }
        if !(200..=299).contains(&status) {
            return Err(Error::WebDav(format!("MKCOL failed with status {}", status)));
        }
        Ok(())
    }

    /// Move/rename a resource (file or directory) on the server using WebDAV MOVE.
    pub async fn move_resource(&self, from: &str, to: &str) -> Result<()> {
        let from_url = self.full_url(from);
        let to_url = self.full_url(to);
        let resp = self.client
            .request(reqwest::Method::from_bytes(b"MOVE").expect("MOVE is a valid HTTP method"), &from_url)
            .basic_auth(self.username.as_str(), Some(self.password.as_str()))
            .header("Destination", &to_url)
            .header("Overwrite", "F")
            .send()
            .await?;

        let status = resp.status().as_u16();
        if status == 412 {
            return Err(Error::WebDav("Destination already exists".into()));
        }
        if !(200..=299).contains(&status) {
            return Err(Error::WebDav(format!("MOVE failed with status {}", status)));
        }
        Ok(())
    }

    /// Ensure a directory exists, creating it and parents as needed.
    pub async fn ensure_dir(&self, path: &str) -> Result<()> {
        let parts: Vec<&str> = path.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
        let mut current = String::new();
        for part in parts {
            current = if current.is_empty() {
                part.to_string()
            } else {
                format!("{}/{}", current, part)
            };
            self.create_dir(&current).await?;
        }
        Ok(())
    }
}

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:resourcetype/>
    <D:getcontentlength/>
    <D:getlastmodified/>
  </D:prop>
</D:propfind>"#;

/// Percent-encode a single path segment (not the whole path).
fn percent_encode(segment: &str) -> String {
    let mut result = String::new();
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

/// Percent-decode a string.
fn percent_decode(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                result.push(val);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).to_string()
}

/// Parse a PROPFIND multistatus XML response into RemoteFileInfo entries.
/// Handles namespace prefix variations (d:, D:, no prefix).
fn parse_propfind_response(xml: &str, base_url: &str, request_path: &str) -> Result<Vec<RemoteFileInfo>> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    let mut results = Vec::new();

    // State machine for parsing
    let mut in_response = false;
    let mut in_propstat = false;
    let mut in_prop = false;
    let mut current_href: Option<String> = None;
    let mut current_is_dir = false;
    let mut current_content_length: u64 = 0;
    let mut current_last_modified: Option<String> = None;
    let mut reading_href = false;
    let mut reading_content_length = false;
    let mut reading_last_modified = false;
    let mut in_resourcetype = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let local = local_name(&name_bytes);
                match local {
                    "response" => {
                        in_response = true;
                        current_href = None;
                        current_is_dir = false;
                        current_content_length = 0;
                        current_last_modified = None;
                    }
                    "propstat" => in_propstat = true,
                    "prop" if in_propstat => in_prop = true,
                    "href" if in_response => reading_href = true,
                    "resourcetype" if in_prop => in_resourcetype = true,
                    "collection" if in_resourcetype => current_is_dir = true,
                    "getcontentlength" if in_prop => reading_content_length = true,
                    "getlastmodified" if in_prop => reading_last_modified = true,
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let local = local_name(&name_bytes);
                match local {
                    "response" => {
                        if let Some(href) = current_href.take() {
                            let path = extract_relative_path(&href, base_url, request_path);
                            if !path.is_empty() {
                                results.push(RemoteFileInfo {
                                    path,
                                    is_dir: current_is_dir,
                                    content_length: current_content_length,
                                    last_modified: current_last_modified.take(),
                                });
                            }
                        }
                        in_response = false;
                    }
                    "propstat" => in_propstat = false,
                    "prop" => in_prop = false,
                    "resourcetype" => in_resourcetype = false,
                    "href" => reading_href = false,
                    "getcontentlength" => reading_content_length = false,
                    "getlastmodified" => reading_last_modified = false,
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if let Ok(text) = e.unescape() {
                    let text = text.to_string();
                    if reading_href {
                        current_href = Some(text);
                    } else if reading_content_length {
                        current_content_length = text.trim().parse().unwrap_or(0);
                    } else if reading_last_modified {
                        current_last_modified = Some(text);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::WebDav(format!("XML parse error: {}", e))),
            _ => {}
        }
    }

    Ok(results)
}

/// Get local name from a potentially namespaced XML tag name.
fn local_name(name: &[u8]) -> &str {
    let s = std::str::from_utf8(name).unwrap_or("");
    // Handle both "D:href" and "href" and "{DAV:}href" forms
    if let Some(pos) = s.rfind(':') {
        &s[pos + 1..]
    } else if let Some(pos) = s.rfind('}') {
        &s[pos + 1..]
    } else {
        s
    }
}

/// Extract a relative path from an href, stripping the base URL prefix and the request path.
fn extract_relative_path(href: &str, base_url: &str, request_path: &str) -> String {
    let decoded = percent_decode(href);
    // Strip scheme + host if present
    let path = if let Some(pos) = decoded.find("://") {
        let after_scheme = &decoded[pos + 3..];
        if let Some(slash) = after_scheme.find('/') {
            &after_scheme[slash..]
        } else {
            ""
        }
    } else {
        decoded.as_str()
    };

    // Extract the base path from base_url
    let base_path = if let Some(pos) = base_url.find("://") {
        let after_scheme = &base_url[pos + 3..];
        if let Some(slash) = after_scheme.find('/') {
            &after_scheme[slash..]
        } else {
            ""
        }
    } else {
        ""
    };

    let mut relative = path.to_string();
    // Strip base path prefix
    if !base_path.is_empty() {
        let bp = base_path.trim_end_matches('/');
        if let Some(stripped) = relative.strip_prefix(bp) {
            relative = stripped.to_string();
        }
    }

    // Strip request path prefix
    let req = request_path.trim_matches('/');
    if !req.is_empty() {
        let prefixed = format!("/{}", req);
        if let Some(stripped) = relative.strip_prefix(&prefixed) {
            relative = stripped.to_string();
        }
    }

    // Clean up leading/trailing slashes
    let relative = relative.trim_matches('/').to_string();
    relative
}

// --- Credential Storage ---

#[cfg(feature = "keyring-storage")]
/// Store WebDAV credentials in the platform keychain. Password is scoped by domain+username
/// to prevent collisions when multiple accounts exist on the same server.
pub fn store_credentials(domain: &str, username: &str, password: &str) -> Result<()> {
    let service = format!("com.onyx.webdav.{}", domain);
    let scoped_service = format!("com.onyx.webdav.{}::{}", domain, username);

    let user_entry = keyring::Entry::new(&service, "username")
        .map_err(|e| Error::Credential(format!("Failed to create keyring entry: {}", e)))?;
    user_entry.set_password(username)
        .map_err(|e| Error::Credential(format!("Failed to store username: {}", e)))?;

    let pass_entry = keyring::Entry::new(&scoped_service, "password")
        .map_err(|e| Error::Credential(format!("Failed to create keyring entry: {}", e)))?;
    pass_entry.set_password(password)
        .map_err(|e| Error::Credential(format!("Failed to store password: {}", e)))?;

    // Clean up legacy unscoped password entry if present
    if let Ok(legacy) = keyring::Entry::new(&service, "password") {
        let _ = legacy.delete_credential();
    }

    Ok(())
}

#[cfg(not(feature = "keyring-storage"))]
/// Store WebDAV credentials (not available without keyring-storage feature).
pub fn store_credentials(_domain: &str, _username: &str, _password: &str) -> Result<()> {
    Err(Error::Credential("Credential storage not available on this platform".into()))
}

#[cfg(feature = "keyring-storage")]
/// Load WebDAV credentials from the platform keychain, falling back to env vars.
pub fn load_credentials(domain: &str) -> Result<(Zeroizing<String>, Zeroizing<String>)> {
    let service = format!("com.onyx.webdav.{}", domain);

    let user_entry = keyring::Entry::new(&service, "username")
        .map_err(|e| Error::Credential(format!("Failed to create keyring entry: {}", e)))?;

    if let Ok(user) = user_entry.get_password() {
        // Try scoped password key first (domain+username), fall back to legacy unscoped key
        let scoped_service = format!("com.onyx.webdav.{}::{}", domain, user);
        let found = keyring::Entry::new(&scoped_service, "password")
            .ok()
            .and_then(|e| e.get_password().ok())
            .map(|p| (p, false))
            .or_else(|| {
                // Migration fallback: try legacy unscoped password entry
                keyring::Entry::new(&service, "password")
                    .ok()
                    .and_then(|e| e.get_password().ok())
                    .map(|p| (p, true))
            });

        if let Some((pass, needs_migration)) = found {
            // Auto-migrate legacy credentials to scoped format
            if needs_migration {
                if let Ok(entry) = keyring::Entry::new(&scoped_service, "password") {
                    let _ = entry.set_password(&pass);
                }
                if let Ok(legacy) = keyring::Entry::new(&service, "password") {
                    let _ = legacy.delete_credential();
                }
            }
            return Ok((Zeroizing::new(user), Zeroizing::new(pass)));
        }
    }

    // Fallback to env vars for headless/CI environments
    if let (Ok(user), Ok(pass)) = (
        std::env::var("ONYX_WEBDAV_USER"),
        std::env::var("ONYX_WEBDAV_PASS"),
    ) {
        log::warn!("Using environment variables for WebDAV credentials — prefer keyring for better security");
        return Ok((Zeroizing::new(user), Zeroizing::new(pass)));
    }

    Err(Error::Credential(format!(
        "No credentials found for '{}'. Run setup or configure environment variables.",
        domain
    )))
}

#[cfg(not(feature = "keyring-storage"))]
/// Load WebDAV credentials from env vars only (keyring not available).
pub fn load_credentials(domain: &str) -> Result<(Zeroizing<String>, Zeroizing<String>)> {
    if let (Ok(user), Ok(pass)) = (
        std::env::var("ONYX_WEBDAV_USER"),
        std::env::var("ONYX_WEBDAV_PASS"),
    ) {
        log::warn!("Using environment variables for WebDAV credentials — these are visible to other processes on this system");
        return Ok((Zeroizing::new(user), Zeroizing::new(pass)));
    }

    Err(Error::Credential(format!(
        "No credentials found for '{}'. Configure environment variables.",
        domain
    )))
}

#[cfg(feature = "keyring-storage")]
/// Delete WebDAV credentials from the platform keychain.
pub fn delete_credentials(domain: &str) -> Result<()> {
    let service = format!("com.onyx.webdav.{}", domain);

    // Load username first so we can delete the scoped password entry
    let username = keyring::Entry::new(&service, "username")
        .ok()
        .and_then(|e| e.get_password().ok());

    if let Some(user) = &username {
        let scoped_service = format!("com.onyx.webdav.{}::{}", domain, user);
        if let Ok(entry) = keyring::Entry::new(&scoped_service, "password") {
            let _ = entry.delete_credential();
        }
    }

    // Clean up legacy unscoped password and username entries
    if let Ok(entry) = keyring::Entry::new(&service, "password") {
        let _ = entry.delete_credential();
    }
    if let Ok(entry) = keyring::Entry::new(&service, "username") {
        let _ = entry.delete_credential();
    }

    Ok(())
}

#[cfg(not(feature = "keyring-storage"))]
/// Delete WebDAV credentials (no-op without keyring-storage feature).
pub fn delete_credentials(_domain: &str) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- URL encoding tests ---

    #[test]
    fn test_percent_encode_simple() {
        assert_eq!(percent_encode("hello"), "hello");
    }

    #[test]
    fn test_percent_encode_spaces() {
        assert_eq!(percent_encode("Buy groceries"), "Buy%20groceries");
    }

    #[test]
    fn test_percent_encode_special_chars() {
        assert_eq!(percent_encode("task (1)"), "task%20%281%29");
    }

    #[test]
    fn test_percent_decode_roundtrip() {
        let original = "Buy groceries (urgent)";
        let encoded = percent_encode(original);
        let decoded = percent_decode(&encoded);
        assert_eq!(decoded, original);
    }

    // --- PROPFIND XML parsing tests ---

    #[test]
    fn test_parse_propfind_with_d_prefix() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/remote/</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/></d:resourcetype>
        <d:getcontentlength>0</d:getcontentlength>
      </d:prop>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/remote/My%20Tasks/</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/></d:resourcetype>
        <d:getcontentlength>0</d:getcontentlength>
      </d:prop>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/remote/My%20Tasks/Buy%20groceries.md</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype/>
        <d:getcontentlength>150</d:getcontentlength>
        <d:getlastmodified>Mon, 01 Jan 2026 00:00:00 GMT</d:getlastmodified>
      </d:prop>
    </d:propstat>
  </d:response>
</d:multistatus>"#;

        let results = parse_propfind_response(xml, "http://example.com/remote", "").unwrap();
        assert_eq!(results.len(), 2); // Root directory itself is empty path -> skipped
        assert_eq!(results[0].path, "My Tasks");
        assert!(results[0].is_dir);
        assert_eq!(results[1].path, "My Tasks/Buy groceries.md");
        assert!(!results[1].is_dir);
        assert_eq!(results[1].content_length, 150);
    }

    #[test]
    fn test_parse_propfind_with_uppercase_d_prefix() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/dav/</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype><D:collection/></D:resourcetype>
      </D:prop>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav/notes.md</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype/>
        <D:getcontentlength>42</D:getcontentlength>
      </D:prop>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

        let results = parse_propfind_response(xml, "http://example.com/dav", "").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "notes.md");
        assert!(!results[0].is_dir);
        assert_eq!(results[0].content_length, 42);
    }

    #[test]
    fn test_parse_propfind_no_prefix() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<multistatus xmlns="DAV:">
  <response>
    <href>/files/</href>
    <propstat>
      <prop>
        <resourcetype><collection/></resourcetype>
      </prop>
    </propstat>
  </response>
  <response>
    <href>/files/test.md</href>
    <propstat>
      <prop>
        <resourcetype/>
        <getcontentlength>100</getcontentlength>
        <getlastmodified>Tue, 15 Mar 2026 10:30:00 GMT</getlastmodified>
      </prop>
    </propstat>
  </response>
</multistatus>"#;

        let results = parse_propfind_response(xml, "http://example.com/files", "").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "test.md");
        assert_eq!(results[0].last_modified.as_deref(), Some("Tue, 15 Mar 2026 10:30:00 GMT"));
    }

    #[test]
    fn test_parse_propfind_with_subpath() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/remote/My%20Tasks/</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/></d:resourcetype>
      </d:prop>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/remote/My%20Tasks/task1.md</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype/>
        <d:getcontentlength>50</d:getcontentlength>
      </d:prop>
    </d:propstat>
  </d:response>
</d:multistatus>"#;

        let results = parse_propfind_response(xml, "http://example.com/remote", "My Tasks").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "task1.md");
    }

    // --- WebDavClient URL building ---

    #[test]
    fn test_new_rejects_http() {
        let result = WebDavClient::new("http://example.com/dav", "user", "pass");
        assert!(result.is_err());
    }

    #[test]
    fn test_new_accepts_https() {
        let result = WebDavClient::new("https://example.com/dav", "user", "pass");
        assert!(result.is_ok());
    }

    #[test]
    fn test_full_url_building() {
        let client = WebDavClient::new_unchecked("http://example.com/dav/", "user", "pass").unwrap();
        assert_eq!(client.full_url(""), "http://example.com/dav");
        assert_eq!(client.full_url("file.md"), "http://example.com/dav/file.md");
        assert_eq!(client.full_url("My Tasks/Buy groceries.md"), "http://example.com/dav/My%20Tasks/Buy%20groceries.md");
    }

    #[test]
    fn test_full_url_strips_leading_slash() {
        let client = WebDavClient::new_unchecked("http://example.com/dav", "user", "pass").unwrap();
        assert_eq!(client.full_url("/file.md"), "http://example.com/dav/file.md");
    }

    // --- extract_relative_path ---

    #[test]
    fn test_extract_relative_path_full_url_href() {
        let path = extract_relative_path(
            "http://example.com/dav/My%20Tasks/file.md",
            "http://example.com/dav",
            "",
        );
        assert_eq!(path, "My Tasks/file.md");
    }

    #[test]
    fn test_extract_relative_path_absolute_href() {
        let path = extract_relative_path(
            "/dav/Work/meeting.md",
            "http://example.com/dav",
            "",
        );
        assert_eq!(path, "Work/meeting.md");
    }
}
