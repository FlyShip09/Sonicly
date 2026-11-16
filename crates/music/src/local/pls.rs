//! PLS playlists, the INI-style lists Winamp introduced: `File1=…`, `Title1=…`, `Length1=…`.

use std::path::{Path, PathBuf};

use super::m3u;

/// The files a PLS playlist names, in the order of their numbers. Paths and `file://` URLs are
/// read as an M3U's are, against `folder`, the folder the playlist sits in. Titles, lengths and
/// web addresses are left out.
pub fn read(bytes: &[u8], folder: &Path) -> Vec<PathBuf> {
    let text = m3u::decode(bytes);
    let mut files: Vec<(u32, PathBuf)> = text
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let number = numbered(key.trim())?;
            Some((number, m3u::located(value.trim(), folder)?))
        })
        .collect();
    // A stable sort, so a file that repeats a number keeps its place in the file.
    files.sort_by_key(|(number, _)| *number);
    files.into_iter().map(|(_, path)| path).collect()
}

/// The number of a `FileN` key, whatever its case.
fn numbered(key: &str) -> Option<u32> {
    let prefix = key.get(..4)?;
    match prefix.eq_ignore_ascii_case("file") {
        true => key[4..].parse().ok(),
        false => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_files_in_the_order_of_their_numbers() {
        let folder = Path::new("music");
        let text = "[playlist]\r\nFile2=b.mp3\r\nTitle2=B\r\nfile1=a.mp3\r\nLength1=12\r\n\
                    NumberOfEntries=2\r\nVersion=2\r\n";
        assert_eq!(
            read(text.as_bytes(), folder),
            [folder.join("a.mp3"), folder.join("b.mp3")]
        );
    }

    #[test]
    fn reads_a_file_url_with_a_byte_order_mark_and_skips_a_stream() {
        let folder = Path::new("music");
        let text =
            "\u{feff}[playlist]\nFile1=file:///tmp/a%20b.mp3\nFile2=http://radio.example/live\n";
        let read = read(text.as_bytes(), folder);
        assert_eq!(read.len(), 1);
        assert!(read[0].ends_with("a b.mp3"));
    }

    #[test]
    fn a_key_that_only_starts_like_a_file_is_not_one() {
        assert_eq!(numbered("File12"), Some(12));
        assert_eq!(numbered("FILE3"), Some(3));
        assert_eq!(numbered("File"), None);
        assert_eq!(numbered("Filex"), None);
        assert_eq!(numbered("Title1"), None);
    }
}
