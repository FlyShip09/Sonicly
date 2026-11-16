//! The version stamped into `settings.json` and the steps that bring an older file up to it, so
//! a release can rename or retype a key without anyone losing their settings.

use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value};

use super::{Values, json, write_settings};

/// One change of shape between two versions of `settings.json`. It renames or retypes keys of
/// the file's top-level object in place, and leaves alone what it does not know.
type Step = fn(&mut Map<String, Value>);

/// Every change of shape so far, oldest first: `STEPS[0]` takes a version 1 file to version 2.
/// To rename or retype a key, append a step. A step that has shipped is never edited.
const STEPS: &[Step] = &[thirteen];

/// The shape this build reads and writes, stamped into the file as `version`.
pub(super) const CURRENT: u32 = STEPS.len() as u32 + 1;

/// The shape of a file with no `version` key, which is how Sonicly wrote it before versions.
const FIRST: u32 = 1;

/// The base text size every file carried before the default moved to 13, and the new default.
const OLD_DEFAULT_FONT: f64 = 14.;
const NEW_DEFAULT_FONT: f64 = 13.;

/// The `version` key of a settings file, read on its own before the rest is.
#[derive(Deserialize)]
struct Stamp {
    #[serde(default = "first")]
    version: u32,
}

fn first() -> u32 {
    FIRST
}

/// Reads `settings.json` as this build's [`Values`], first bringing a file an older Sonicly
/// wrote up to today's shape. Unknown keys are ignored, so a file from a newer Sonicly loads
/// with whatever this build understands. An error carries the line of the mistake, except for
/// a mistyped value in a file that needed a step, which reads as line 0.
pub(super) fn parse(bytes: &[u8]) -> serde_json::Result<Values> {
    let bytes = json(bytes);
    let from = serde_json::from_slice::<Stamp>(bytes)?.version;
    let mut values: Values = match from >= CURRENT {
        true => serde_json::from_slice(bytes)?,
        false => serde_json::from_value(migrate(STEPS, from, serde_json::from_slice(bytes)?))?,
    };
    values.version = CURRENT;
    Ok(values)
}

/// Applies every step after version `from` to a parsed settings file, in order. A file at or
/// past the newest version comes back untouched.
fn migrate(steps: &[Step], from: u32, mut value: Value) -> Value {
    if let Some(keys) = value.as_object_mut() {
        // nothing ever wrote version 0, so a hand-typed one is read as the first shape
        for step in steps.iter().skip(from.saturating_sub(FIRST) as usize) {
            step(keys);
        }
    }
    value
}

/// Version 1 to 2: the default text size went from 14 to 13 px. The size is always written, so a
/// file that holds the old default moves with it; any other size was a choice and stays.
fn thirteen(keys: &mut Map<String, Value>) {
    let Some(appearance) = keys.get_mut("appearance").and_then(Value::as_object_mut) else {
        return;
    };
    if appearance.get("font_size").and_then(Value::as_f64) == Some(OLD_DEFAULT_FONT) {
        appearance.insert("font_size".to_owned(), Value::from(NEW_DEFAULT_FONT));
    }
}

/// Keeps a copy of a settings file that a newer Sonicly wrote, as `settings.v<version>.json`,
/// because this build's next save drops the keys it does not know. A copy already there is
/// left alone, so it stays the newer build's own file.
pub(super) fn keep_newer(path: &Path, bytes: &[u8]) {
    let Ok(Stamp { version }) = serde_json::from_slice(json(bytes)) else {
        return;
    };
    if version <= CURRENT {
        return;
    }
    let copy = path.with_file_name(format!("settings.v{version}.json"));
    if copy.exists() {
        return;
    }
    match write_settings(&copy, bytes) {
        Ok(()) => log::warn!(
            "settings: {} is from a newer Sonicly, kept a copy as {}",
            path.display(),
            copy.display()
        ),
        Err(error) => log::warn!("settings: cannot keep {}: {error:#}", copy.display()),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    fn renamed(keys: &mut Map<String, Value>) {
        if let Some(value) = keys.remove("old") {
            keys.insert("new".to_owned(), value);
        }
    }

    fn retyped(keys: &mut Map<String, Value>) {
        let number = keys
            .get("new")
            .and_then(Value::as_str)
            .and_then(|text| text.parse::<u32>().ok());
        if let Some(number) = number {
            keys.insert("new".to_owned(), json!(number));
        }
    }

    #[test]
    fn an_older_file_takes_every_step_after_its_version() {
        let steps: &[Step] = &[renamed, retyped];

        let first = json!({ "old": "3", "kept": true });
        assert_eq!(
            migrate(steps, 1, first.clone()),
            json!({ "new": 3, "kept": true })
        );
        assert_eq!(migrate(steps, 0, first.clone()), migrate(steps, 1, first));
        // a version 2 file was written after the rename, so only the retype is left
        assert_eq!(
            migrate(steps, 2, json!({ "old": 1, "new": "3" })),
            json!({ "old": 1, "new": 3 })
        );
    }

    #[test]
    fn the_old_default_text_size_moves_to_thirteen() {
        let mut moved = json!({ "appearance": { "font_size": 14.0, "theme": "dark" } });
        thirteen(moved.as_object_mut().unwrap());
        assert_eq!(
            moved,
            json!({ "appearance": { "font_size": 13.0, "theme": "dark" } })
        );

        let mut chosen = json!({ "appearance": { "font_size": 16.0 } });
        thirteen(chosen.as_object_mut().unwrap());
        assert_eq!(chosen, json!({ "appearance": { "font_size": 16.0 } }));

        let mut bare = json!({ "gapless": true });
        thirteen(bare.as_object_mut().unwrap());
        assert_eq!(bare, json!({ "gapless": true }));
    }

    #[test]
    fn a_current_or_newer_file_is_left_as_it_is() {
        let steps: &[Step] = &[renamed, retyped];
        let file = json!({ "old": "3" });

        assert_eq!(migrate(steps, 3, file.clone()), file);
        assert_eq!(migrate(steps, 99, file.clone()), file);
        assert_eq!(migrate(steps, 1, json!([1, 2])), json!([1, 2]));
    }

    #[test]
    fn a_file_without_a_version_is_the_first_shape() {
        let stamp: Stamp = serde_json::from_str(r#"{ "gapless": false }"#).unwrap();
        assert_eq!(stamp.version, FIRST);

        let values = parse(br#"{ "gapless": false }"#).unwrap();
        assert!(!values.gapless);
        assert_eq!(values.version, CURRENT);
    }

    #[test]
    fn a_newer_file_with_unknown_keys_still_loads() {
        let values = parse(
            br#"{ "version": 99, "gapless": false, "from_a_later_release": { "a": [1, 2] } }"#,
        )
        .unwrap();

        assert!(!values.gapless);
        assert_eq!(values.version, CURRENT);
    }

    #[test]
    fn a_file_from_before_the_app_had_its_own_name_still_loads() {
        let values = parse(br#"{ "discord_name": "sonora", "gapless": false }"#).unwrap();
        assert_eq!(values.discord_name, crate::settings::DiscordName::Sonicly);
        assert!(!values.gapless);
    }

    #[test]
    fn the_version_is_the_first_key_written() {
        let written = serde_json::to_string(&Values::default()).unwrap();
        assert!(written.starts_with(&format!("{{\"version\":{CURRENT},")));
    }

    #[test]
    fn a_broken_file_reports_the_line_of_the_mistake() {
        let error = parse(b"{\n  \"gapless\": maybe\n}").unwrap_err();
        assert_eq!(error.line(), 2);

        let mistyped = format!("{{\n  \"version\": {CURRENT},\n  \"gapless\": \"yes\"\n}}");
        let mistyped = parse(mistyped.as_bytes()).unwrap_err();
        assert_eq!(mistyped.line(), 3);
    }

    #[test]
    fn a_newer_file_is_copied_aside_once() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.json");
        let copy = folder.path().join("settings.v99.json");
        let newer = br#"{ "version": 99, "later": true }"#;

        keep_newer(&path, br#"{ "gapless": false }"#);
        keep_newer(&path, format!("{{ \"version\": {CURRENT} }}").as_bytes());
        assert!(fs::read_dir(folder.path()).unwrap().next().is_none());

        keep_newer(&path, newer);
        assert_eq!(fs::read(&copy).unwrap(), newer);

        keep_newer(&path, br#"{ "version": 99, "later": false }"#);
        assert_eq!(fs::read(&copy).unwrap(), newer);
    }
}
