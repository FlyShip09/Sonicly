use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result};
use http::header;
use serde::{Deserialize, Serialize};

const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const FILE: &str = "pathfinder.json";

/// Operations read from the web player bundle whenever it is fetched, whichever one was asked
/// for. Sonicly contacts no third-party hash service, so the bundle is the only source and any
/// other operation is looked up in it by name.
const DESKTOP_OPERATIONS: [(&str, &str); 4] = [
    ("home", "query"),
    ("libraryV3", "query"),
    ("pinLibraryItem", "mutation"),
    ("unpinLibraryItem", "mutation"),
];

pub(super) struct Hash {
    pub(super) value: String,
    pub(super) tried: bool,
}

#[derive(Clone, Deserialize, Serialize)]
struct Registry {
    fetched: u64,
    operations: HashMap<String, String>,
}

pub(super) async fn resolve(operation: &str) -> Result<Hash> {
    let cached = registry();
    if let Some(value) = cached
        .as_ref()
        .filter(|registry| aged(registry) < MAX_AGE)
        .and_then(|registry| registry.operations.get(operation).cloned())
    {
        return Ok(Hash {
            value,
            tried: false,
        });
    }
    match desktop_hash(operation).await {
        Ok(value) => Ok(Hash {
            value,
            tried: false,
        }),
        Err(error) => cached
            .and_then(|mut registry| registry.operations.remove(operation))
            .map(|value| Hash { value, tried: true })
            .ok_or(error),
    }
}

pub(super) async fn refetch(operation: &str) -> Option<String> {
    desktop_hash(operation).await.ok()
}

fn sane(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn aged(registry: &Registry) -> Duration {
    Duration::from_secs(now().saturating_sub(registry.fetched))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn registry() -> Option<Registry> {
    cache().lock().ok()?.clone()
}

fn store(operations: &HashMap<String, String>) {
    let registry = Registry {
        fetched: now(),
        operations: operations.clone(),
    };
    write(&registry);
    if let Ok(mut cache) = cache().lock() {
        *cache = Some(registry);
    }
}

fn cache() -> &'static Mutex<Option<Registry>> {
    static CACHE: OnceLock<Mutex<Option<Registry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(read()))
}

fn read() -> Option<Registry> {
    let body = std::fs::read(path()).ok()?;
    serde_json::from_slice(&body).ok()
}

fn write(registry: &Registry) {
    let path = path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let written = serde_json::to_vec_pretty(registry)
        .context("cannot encode")
        .and_then(|body| std::fs::write(&path, body).context("cannot save"));
    if let Err(error) = written {
        log::warn!("pathfinder: {error:#} {}", path.display());
    }
}

fn path() -> PathBuf {
    crate::credentials::root().join(FILE)
}

// Discover an operation in the web player bundle.
// shortcut: one bundle download per operation not yet cached, harvest every hash at once if that
// proves slow
async fn desktop_hash(operation: &str) -> Result<String> {
    // librespot overwrites User-Agent on every request, which makes this page serve
    // the mobile bundle. This unauthenticated client reads only public web assets.
    let public = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36")
        .timeout(Duration::from_secs(20)).build()?;
    let html = public_text(&public, "https://open.spotify.com").await?;
    let bundle = desktop_bundle(&html).context("Spotify page has no desktop player bundle")?;
    let javascript = public_text(&public, bundle).await?;
    let mut operations = registry()
        .map(|registry| registry.operations)
        .unwrap_or_default();
    let mut requested = None;
    for (name, kind) in DESKTOP_OPERATIONS {
        if let Some(hash) = operation_hash(&javascript, name, kind) {
            operations.insert(name.to_owned(), hash.to_owned());
            if name == operation {
                requested = Some(hash.to_owned());
            }
        }
    }
    let requested = requested.or_else(|| {
        let hash = ["query", "mutation"]
            .into_iter()
            .find_map(|kind| operation_hash(&javascript, operation, kind))?;
        operations.insert(operation.to_owned(), hash.to_owned());
        Some(hash.to_owned())
    });
    let requested =
        requested.with_context(|| format!("Spotify bundle has no {operation} operation"))?;
    store(&operations);
    Ok(requested)
}

async fn public_text(client: &reqwest::Client, url: &str) -> Result<String> {
    use std::io::Read as _;
    let body = client
        .get(url)
        .header(header::ACCEPT_ENCODING, "identity")
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let bytes = if body.starts_with(&[0x1f, 0x8b]) {
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(body.as_ref()).read_to_end(&mut decoded)?;
        decoded
    } else {
        body.to_vec()
    };
    String::from_utf8(bytes).context("Spotify bundle is not UTF-8")
}

fn desktop_bundle(html: &str) -> Option<&str> {
    html.split('"').find(|url| {
        url.starts_with("https://open.spotifycdn.com/cdn/build/web-player/web-player.")
            && url.ends_with(".js")
    })
}

fn operation_hash<'a>(javascript: &'a str, operation: &str, kind: &str) -> Option<&'a str> {
    let marker = format!("\"{operation}\",\"{kind}\",\"");
    let (_, after) = javascript.split_once(&marker)?;
    let (hash, _) = after.split_once('"')?;
    sane(hash).then_some(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_only_the_desktop_bundle_and_a_valid_query_hash() {
        let url = "https://open.spotifycdn.com/cdn/build/web-player/web-player.abc.js";
        assert_eq!(
            desktop_bundle(&format!(r#"<script src="{url}"></script>"#)),
            Some(url)
        );
        assert!(
            desktop_bundle(r#"<script src="https://example.com/web-player.js"></script>"#)
                .is_none()
        );
        let hash = "0123456789abcdef".repeat(4);
        assert_eq!(
            operation_hash(
                &format!(r#"new Query("libraryV3","query","{hash}",null)"#),
                "libraryV3",
                "query"
            ),
            Some(hash.as_str())
        );
        assert!(
            operation_hash(
                r#"new Query("libraryV3","query","broken",null)"#,
                "libraryV3",
                "query"
            )
            .is_none()
        );
    }

    #[test]
    fn discovers_each_library_mutation_without_confusing_pin_and_unpin() {
        let pin = "a".repeat(64);
        let unpin = "b".repeat(64);
        let javascript = format!(
            r#"new Query("unpinLibraryItem","mutation","{unpin}",null);new Query("pinLibraryItem","mutation","{pin}",null)"#
        );
        assert_eq!(
            operation_hash(&javascript, "pinLibraryItem", "mutation"),
            Some(pin.as_str())
        );
        assert_eq!(
            operation_hash(&javascript, "unpinLibraryItem", "mutation"),
            Some(unpin.as_str())
        );
        assert!(operation_hash(&javascript, "pinLibraryItem", "query").is_none());
    }
}
