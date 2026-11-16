use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result, bail};
use http::header;
use serde::{Deserialize, Serialize};

const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const FILE: &str = "pathfinder.json";

/// The page that names the web player bundle.
const PAGE: &str = "https://open.spotify.com";
/// The folder on Spotify's CDN every script of the web player is read from.
const BUILD: &str = "https://open.spotifycdn.com/cdn/build/web-player/";
/// The only hosts a lookup contacts, redirects included. Sonicly asks no third-party hash
/// service, so the web player's own scripts are the only source.
const HOSTS: [&str; 2] = ["open.spotify.com", "open.spotifycdn.com"];
/// How many redirects a request follows.
const REDIRECTS: usize = 5;
/// Words in the names of the chunks whose queries the main bundle leaves out: the search page,
/// which also holds the genre grid, and a genre's own page.
const ROUTES: [&str; 2] = ["search", "browse"];
/// The most chunks one lookup reads.
const MAX_CHUNKS: usize = 8;
/// The most bytes taken from the page or the main bundle, and from one chunk.
const MAX_BUNDLE: usize = 16 * 1024 * 1024;
const MAX_CHUNK: usize = 2 * 1024 * 1024;

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

/// Finds an operation's hash in the web player's scripts and keeps every other hash those
/// scripts declare. The main bundle is read first. An operation it does not declare belongs to
/// a page the player loads later, so the chunks [`wanted`] picks are read as well, each from
/// [`BUILD`] and nowhere else.
// shortcut: the page and the main bundle are downloaded again for each operation not yet
// cached, keep the bundle between lookups if that proves slow
async fn desktop_hash(operation: &str) -> Result<String> {
    // librespot overwrites User-Agent on every request, which makes this page serve
    // the mobile bundle. This unauthenticated client reads only public web assets.
    let public = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36")
        .redirect(reqwest::redirect::Policy::custom(spotify_only))
        .timeout(Duration::from_secs(20)).build()?;
    let html = public_text(&public, PAGE, MAX_BUNDLE).await?;
    let bundle = desktop_bundle(&html).context("Spotify page has no desktop player bundle")?;
    let javascript = public_text(&public, bundle, MAX_BUNDLE).await?;
    let mut found = HashMap::new();
    harvest(&javascript, &mut found);
    if !found.contains_key(operation) {
        for (name, hash) in wanted(&chunks(&javascript), operation) {
            match public_text(&public, &format!("{BUILD}{name}.{hash}.js"), MAX_CHUNK).await {
                Ok(chunk) => harvest(&chunk, &mut found),
                Err(error) => log::debug!("pathfinder: cannot read the {name} chunk: {error:#}"),
            }
        }
    }
    let requested = found
        .get(operation)
        .cloned()
        .with_context(|| format!("Spotify bundle has no {operation} operation"))?;
    let mut operations = registry()
        .map(|registry| registry.operations)
        .unwrap_or_default();
    operations.extend(found);
    store(&operations);
    Ok(requested)
}

/// Follows a redirect only while it stays on https and on one of [`HOSTS`].
fn spotify_only(attempt: reqwest::redirect::Attempt) -> reqwest::redirect::Action {
    let url = attempt.url();
    if url.scheme() != "https" || !url.host_str().is_some_and(|host| HOSTS.contains(&host)) {
        attempt.error("redirect leaves Spotify's web player")
    } else if attempt.previous().len() > REDIRECTS {
        attempt.error("too many redirects")
    } else {
        attempt.follow()
    }
}

/// The body of a public web player asset as text. One that runs past `limit` bytes is refused,
/// before and after unpacking.
async fn public_text(client: &reqwest::Client, url: &str, limit: usize) -> Result<String> {
    use std::io::Read as _;
    let mut response = client
        .get(url)
        .header(header::ACCEPT_ENCODING, "identity")
        .send()
        .await?
        .error_for_status()?;
    let mut body = Vec::new();
    while let Some(part) = response.chunk().await? {
        if body.len() + part.len() > limit {
            bail!("Spotify asset is larger than {limit} bytes");
        }
        body.extend_from_slice(&part);
    }
    let bytes = if body.starts_with(&[0x1f, 0x8b]) {
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(body.as_slice())
            .take(limit as u64 + 1)
            .read_to_end(&mut decoded)?;
        if decoded.len() > limit {
            bail!("Spotify asset unpacks to more than {limit} bytes");
        }
        decoded
    } else {
        body
    };
    String::from_utf8(bytes).context("Spotify bundle is not UTF-8")
}

fn desktop_bundle(html: &str) -> Option<&str> {
    html.split('"').find(|url| {
        url.strip_prefix(BUILD)
            .is_some_and(|file| file.starts_with("web-player.") && file.ends_with(".js"))
    })
}

/// Adds every operation `javascript` declares to `found`, keeping the ones already there.
fn harvest(javascript: &str, found: &mut HashMap<String, String>) {
    for (name, hash) in declared(javascript) {
        found
            .entry(name.to_owned())
            .or_insert_with(|| hash.to_owned());
    }
}

/// Every operation `javascript` declares, as its name and hash. The player writes one as
/// `"name","query","hash"` or `"name","mutation","hash"`. A hash that is not 64 hex digits is
/// left out, and so is a name that is not a plain identifier.
fn declared(javascript: &str) -> impl Iterator<Item = (&str, &str)> {
    ["\",\"query\",\"", "\",\"mutation\",\""]
        .into_iter()
        .flat_map(move |kind| {
            javascript
                .match_indices(kind)
                .filter_map(move |(at, kind)| {
                    let name = javascript[..at].rsplit('"').next()?;
                    let hash = javascript[at + kind.len()..].split('"').next()?;
                    let plain = !name.is_empty()
                        && name.len() <= 64
                        && name
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
                    (plain && sane(hash)).then_some((name, hash))
                })
        })
}

/// The chunks the player loads later, each as the name and the content hash its file name is
/// made of. Webpack writes them as two maps by chunk id,
/// `({1:"name"})[e]||e)+"."+({1:"hash",2:"hash"})[e]+".js"`, where a chunk with no name goes by
/// its id. The stylesheet map beside it has the same shape and is passed over. So is any name
/// or hash that [`part`] turns down.
fn chunks(javascript: &str) -> Vec<(&str, &str)> {
    const JOIN: &str = ")+\".\"+({";
    for (at, _) in javascript.match_indices(JOIN) {
        let Some((hashes, rest)) = javascript[at + JOIN.len()..].split_once('}') else {
            continue;
        };
        // `)[e]+".js"`, whatever the minifier called `e`
        let scripts = rest
            .split_once(']')
            .is_some_and(|(index, rest)| index.len() < 8 && rest.starts_with("+\".js\""));
        if !scripts {
            continue;
        }
        let names: HashMap<&str, &str> = javascript[..at]
            .rsplit_once("({")
            .and_then(|(_, names)| names.split_once('}'))
            .map(|(names, _)| pairs(names).collect())
            .unwrap_or_default();
        return pairs(hashes)
            .map(|(id, hash)| (names.get(id).copied().unwrap_or(id), hash))
            .filter(|(name, hash)| part(name) && part(hash))
            .collect();
    }
    Vec::new()
}

/// The entries of a minified object literal, `1:"a",2:"b"`, as each key and its unquoted value.
fn pairs(map: &str) -> impl Iterator<Item = (&str, &str)> {
    map.split(',').filter_map(|entry| {
        let (key, value) = entry.split_once(':')?;
        Some((key, value.strip_prefix('"')?.strip_suffix('"')?))
    })
}

/// Whether `text` can stand in a chunk's file name. Nothing that could end the path or name
/// another host gets through: no dot, slash, colon, percent sign or `@`.
fn part(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'~'))
}

/// The chunks to read for `operation`, [`MAX_CHUNKS`] at most: the ones named after a word of
/// the operation first, then the ones named after one of [`ROUTES`].
fn wanted<'a>(chunks: &[(&'a str, &'a str)], operation: &str) -> Vec<(&'a str, &'a str)> {
    let words = words(operation);
    let own = chunks
        .iter()
        .filter(|(name, _)| words.iter().any(|word| name.contains(word.as_str())));
    let routes = chunks
        .iter()
        .filter(|(name, _)| ROUTES.iter().any(|route| name.contains(route)));
    let mut picked = Vec::new();
    for chunk in own.chain(routes) {
        if picked.len() < MAX_CHUNKS && !picked.contains(chunk) {
            picked.push(*chunk);
        }
    }
    picked
}

/// The words of a camel-case operation name that are four letters or longer, in lower case:
/// `searchDesktop` is `search` and `desktop`, and `getAlbum` is `album`.
fn words(operation: &str) -> Vec<String> {
    let mut words = vec![String::new()];
    for letter in operation.chars() {
        if letter.is_ascii_uppercase() {
            words.push(String::new());
        }
        if let Some(word) = words.last_mut() {
            word.push(letter.to_ascii_lowercase());
        }
    }
    words.retain(|word| word.len() >= 4);
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chunk maps as the main bundle carries them, cut down to a few entries: the script
    /// map, then the stylesheet map of the same shape.
    const RUNTIME: &str = concat!(
        r#"u.u=e=>""+(({1498:"xpui-routes-offline-browse",3719:"browse-v2",4406:"xpui-routes-search","#,
        r#"7001:"../evil",9148:"home-v2"})[e]||e)+"."+({1169:"3dbf307c",1498:"baf146d5","#,
        r#"3719:"e67c81d6",4406:"bdb477e8",7001:"aaaaaaaa",7002:"bb/bb",9148:"ad53f2f5"})[e]+".js","#,
        r#"u.miniCssF=e=>""+(({4406:"xpui-routes-search"})[e]||e)+"."+({4406:"0c0c0c0c"})[e]+".css""#,
    );

    fn harvested(javascript: &str) -> HashMap<String, String> {
        let mut found = HashMap::new();
        harvest(javascript, &mut found);
        found
    }

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
            harvested(&format!(r#"new Query("libraryV3","query","{hash}",null)"#)).get("libraryV3"),
            Some(&hash)
        );
        assert!(harvested(r#"new Query("libraryV3","query","broken",null)"#).is_empty());
    }

    #[test]
    fn discovers_each_library_mutation_without_confusing_pin_and_unpin() {
        let pin = "a".repeat(64);
        let unpin = "b".repeat(64);
        let found = harvested(&format!(
            r#"new Query("unpinLibraryItem","mutation","{unpin}",null);new Query("pinLibraryItem","mutation","{pin}",null)"#
        ));
        assert_eq!(found.get("pinLibraryItem"), Some(&pin));
        assert_eq!(found.get("unpinLibraryItem"), Some(&unpin));
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn harvests_every_operation_of_a_chunk_and_keeps_the_first_hash_of_a_name() {
        let (search, albums, later) = ("1".repeat(64), "2".repeat(64), "3".repeat(64));
        let chunk = format!(
            r#"a=new i.l("searchDesktop","query","{search}",null),b=new i.l("searchAlbums","query","{albums}",null),c=new i.l("searchDesktop","query","{later}",null),d=new i.l("not a name","query","{later}",null)"#
        );
        let mut found = HashMap::new();
        harvest(&chunk, &mut found);
        assert_eq!(found.get("searchDesktop"), Some(&search));
        assert_eq!(found.get("searchAlbums"), Some(&albums));
        assert_eq!(found.len(), 2);

        // a later script never replaces what an earlier one declared
        harvest(
            &format!(r#"new i.l("searchAlbums","query","{later}",null)"#),
            &mut found,
        );
        assert_eq!(found.get("searchAlbums"), Some(&albums));
    }

    #[test]
    fn reads_the_script_chunk_map_and_not_the_stylesheet_one() {
        assert_eq!(
            chunks(RUNTIME),
            [
                ("1169", "3dbf307c"),
                ("xpui-routes-offline-browse", "baf146d5"),
                ("browse-v2", "e67c81d6"),
                ("xpui-routes-search", "bdb477e8"),
                ("home-v2", "ad53f2f5"),
            ]
        );
        assert!(chunks(r#"u.u=e=>e+".js""#).is_empty());
        assert!(chunks(r#"(({1:"a"})[e]||e)+"."+({1:"0c0c0c0c"})[e]+".css""#).is_empty());
    }

    #[test]
    fn a_chunk_name_cannot_leave_the_build_folder() {
        for name in ["../evil", "a/b", "a.b", "evil.com:443", "a@b", "a%2fb", ""] {
            assert!(!part(name), "{name}");
        }
        for name in [
            "xpui-routes-search",
            "encore~web-player",
            "bdb477e8",
            "1169",
        ] {
            assert!(part(name), "{name}");
        }
    }

    #[test]
    fn picks_the_chunks_named_after_the_operation_then_the_search_and_browse_routes() {
        let chunks = chunks(RUNTIME);
        let names = |operation: &str| -> Vec<&str> {
            wanted(&chunks, operation)
                .into_iter()
                .map(|(name, _)| name)
                .collect()
        };
        assert_eq!(
            names("searchDesktop"),
            [
                "xpui-routes-search",
                "xpui-routes-offline-browse",
                "browse-v2"
            ]
        );
        // the genre grid is declared in the search chunk, which no word of its name points at
        assert_eq!(
            names("browseAll"),
            [
                "xpui-routes-offline-browse",
                "browse-v2",
                "xpui-routes-search"
            ]
        );
        assert_eq!(names("home")[0], "home-v2");
        assert_eq!(
            words("queryArtistOverview"),
            ["query", "artist", "overview"]
        );
        assert_eq!(words("libraryV3"), ["library"]);
    }

    #[test]
    fn reads_no_more_chunks_than_the_cap() {
        let hash = "0c0c0c0c";
        let names: Vec<String> = (0..MAX_CHUNKS * 2)
            .map(|index| format!("xpui-routes-search-{index}"))
            .collect();
        let chunks: Vec<(&str, &str)> = names.iter().map(|name| (name.as_str(), hash)).collect();
        assert_eq!(wanted(&chunks, "searchDesktop").len(), MAX_CHUNKS);
    }
}
