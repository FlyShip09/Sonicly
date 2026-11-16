//! XSPF playlists, the XML lists VLC and others write: a `<location>` in each `<track>`.

use std::path::{Path, PathBuf};

use percent_encoding::percent_decode_str;

use super::m3u;

const FILE_URL: &str = "file://";

/// The files an XSPF playlist names, in its order. A location is a URI, so a relative one is
/// percent-decoded before it is read against `folder`, the folder the playlist sits in, and a
/// `file://` one is turned back into a path. Web addresses are left out, and a file that is not
/// well-formed XML names nothing.
pub fn read(bytes: &[u8], folder: &Path) -> Vec<PathBuf> {
    let text = m3u::decode(bytes);
    let document = match roxmltree::Document::parse(&text) {
        Ok(document) => document,
        Err(error) => {
            log::warn!("local: cannot read an xspf playlist: {error}");
            return Vec::new();
        }
    };
    // Names are matched without their namespace, which a hand-written file often leaves out.
    document
        .descendants()
        .filter(|node| node.tag_name().name() == "track")
        .filter_map(|track| {
            let location = track
                .children()
                .find(|node| node.tag_name().name() == "location")?;
            located(location.text()?.trim(), folder)
        })
        .collect()
}

/// The file one location names. A `file://` URL is decoded where it is taken apart.
fn located(location: &str, folder: &Path) -> Option<PathBuf> {
    let url = location
        .get(..FILE_URL.len())
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case(FILE_URL));
    match url {
        true => m3u::located(location, folder),
        false => m3u::located(&percent_decode_str(location).decode_utf8_lossy(), folder),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<playlist version="1" xmlns="http://xspf.org/ns/0/">
  <title>Mix</title>
  <trackList>
    <track><title>One</title><location>one%20song.flac</location></track>
    <track><location>file:///tmp/two%20songs.mp3</location></track>
    <track><location>https://radio.example/live</location></track>
    <track><title>No location</title></track>
  </trackList>
</playlist>"#;

    #[test]
    fn reads_relative_and_file_locations_and_skips_the_rest() {
        let folder = Path::new("music");
        let read = read(LIST.as_bytes(), folder);
        assert_eq!(read.len(), 2);
        assert_eq!(read[0], folder.join("one song.flac"));
        assert!(read[1].ends_with("two songs.mp3"));
    }

    #[test]
    fn reads_a_list_with_a_byte_order_mark_and_no_namespace() {
        let folder = Path::new("music");
        let text = "\u{feff}<playlist><trackList><track><location>a.mp3</location></track>\
                    </trackList></playlist>";
        assert_eq!(read(text.as_bytes(), folder), [folder.join("a.mp3")]);
    }

    #[test]
    fn a_file_that_is_not_xml_names_nothing() {
        assert!(read(b"#EXTM3U\na.mp3\n", Path::new("music")).is_empty());
    }
}
