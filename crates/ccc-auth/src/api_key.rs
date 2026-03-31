//! API-key resolution.
//! Corresponds to TS `getApiKey()` in `src/utils/auth.ts`.
//!
//! Priority order (mirrors TS):
//!   1. ANTHROPIC_API_KEY env (when prefer_3p or CI)
//!   2. File-descriptor (CLAUDE_CODE_API_KEY_FILE_DESCRIPTOR)
//!   3. API-key helper (CLAUDE_CODE_API_KEY_HELPER)
//!   4. Secure storage (keychain / plaintext)

use anyhow::Result;

use crate::{
    storage::read_credentials,
    types::{ApiKeySource, ResolvedApiKey},
};

/// Resolve the best available API key.
/// This is intentionally synchronous and cheap (no network IO).
/// To warm the api-key-helper cache call `run_api_key_helper` first.
pub fn resolve_api_key() -> Result<ResolvedApiKey> {
    // 1. Direct environment variable (always wins in CI)
    if is_ci() {
        if let Some(key) = std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.is_empty()) {
            return Ok(ResolvedApiKey {
                key: Some(key),
                source: ApiKeySource::AnthropicApiKeyEnv,
            });
        }
        // OAuth-only CI case: return None without error
        return Ok(ResolvedApiKey { key: None, source: ApiKeySource::None });
    }

    // 2. ANTHROPIC_API_KEY env (non-CI)
    if let Some(key) = std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.is_empty()) {
        return Ok(ResolvedApiKey {
            key: Some(key),
            source: ApiKeySource::AnthropicApiKeyEnv,
        });
    }

    // 3. File descriptor (CLAUDE_CODE_API_KEY_FILE_DESCRIPTOR)
    if let Some(key) = read_key_from_file_descriptor() {
        return Ok(ResolvedApiKey {
            key: Some(key),
            source: ApiKeySource::FileDescriptor,
        });
    }

    // 4. API-key helper command
    if std::env::var("CLAUDE_CODE_API_KEY_HELPER").is_ok() {
        // Caller should run `run_api_key_helper()` asynchronously to warm the cache.
        // Return the (possibly None) cached result without blocking.
        let cached = API_KEY_HELPER_CACHE
            .lock()
            .unwrap()
            .clone()
            .map(|c| c.value);
        return Ok(ResolvedApiKey {
            key: cached,
            source: ApiKeySource::ApiKeyHelper,
        });
    }

    // 5. Secure storage (keychain / plaintext)
    let data = read_credentials().unwrap_or_default();
    if let Some(key) = data.api_key {
        return Ok(ResolvedApiKey {
            key: Some(key),
            source: ApiKeySource::Keychain,
        });
    }

    Ok(ResolvedApiKey { key: None, source: ApiKeySource::None })
}

// ── File-descriptor helper ────────────────────────────────────────────────────

fn read_key_from_file_descriptor() -> Option<String> {
    use std::{
        io::Read,
        os::unix::io::FromRawFd,
    };

    let fd_str = std::env::var("CLAUDE_CODE_API_KEY_FILE_DESCRIPTOR").ok()?;
    let fd: i32 = fd_str.parse().ok()?;
    // SAFETY: we trust the env var comes from the process that spawned us.
    let mut f = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut buf = String::new();
    f.read_to_string(&mut buf).ok()?;
    let key = buf.trim().to_owned();
    if key.is_empty() { None } else { Some(key) }
}

// ── API-key helper cache ──────────────────────────────────────────────────────

#[derive(Clone)]
struct HelperCacheEntry {
    value: String,
    timestamp_ms: u64,
}

static API_KEY_HELPER_CACHE: std::sync::Mutex<Option<HelperCacheEntry>> =
    std::sync::Mutex::new(None);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn helper_ttl_ms() -> u64 {
    std::env::var("CLAUDE_CODE_API_KEY_HELPER_TTL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5 * 60 * 1_000) // 5 minutes default
}

/// Run the configured API-key helper command and populate the in-process cache.
/// Returns the key string, or an error if the helper fails.
pub async fn run_api_key_helper() -> anyhow::Result<String> {
    let cmd = std::env::var("CLAUDE_CODE_API_KEY_HELPER")
        .map_err(|_| anyhow::anyhow!("CLAUDE_CODE_API_KEY_HELPER not set"))?;

    // Return cached value if still fresh.
    {
        let guard = API_KEY_HELPER_CACHE.lock().unwrap();
        if let Some(ref entry) = *guard {
            if now_ms().saturating_sub(entry.timestamp_ms) < helper_ttl_ms() {
                return Ok(entry.value.clone());
            }
        }
    }

    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("api-key helper spawn failed: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("api-key helper failed: {stderr}");
    }

    let key = String::from_utf8(output.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|_| anyhow::anyhow!("api-key helper output is not valid UTF-8"))?;

    *API_KEY_HELPER_CACHE.lock().unwrap() = Some(HelperCacheEntry {
        value: key.clone(),
        timestamp_ms: now_ms(),
    });

    Ok(key)
}

/// Clear the API-key helper cache (e.g. after a 401).
pub fn clear_api_key_helper_cache() {
    *API_KEY_HELPER_CACHE.lock().unwrap() = None;
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn is_ci() -> bool {
    std::env::var("CI")
        .map(|v| !v.is_empty() && v != "0" && v.to_ascii_lowercase() != "false")
        .unwrap_or(false)
        || std::env::var("NODE_ENV").map(|v| v == "test").unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_ttl_defaults_to_five_minutes() {
        // Without env var, should return 300_000 ms
        std::env::remove_var("CLAUDE_CODE_API_KEY_HELPER_TTL_MS");
        assert_eq!(helper_ttl_ms(), 300_000);
    }

    #[test]
    fn helper_ttl_respects_env_var() {
        std::env::set_var("CLAUDE_CODE_API_KEY_HELPER_TTL_MS", "60000");
        assert_eq!(helper_ttl_ms(), 60_000);
        std::env::remove_var("CLAUDE_CODE_API_KEY_HELPER_TTL_MS");
    }
}
