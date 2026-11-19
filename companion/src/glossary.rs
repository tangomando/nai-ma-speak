//! The glossary: translations the companion remembers, and what the addon is given of
//! them to show in the game's own chat.
//!
//! Players say the same things again and again (缺坦, 3q, 等我一下). Each chat line the
//! companion translates is kept in `glossary.json` beside the settings, and a line seen
//! before is answered from there first, at once and without asking the backend.
//!
//! The addon cannot be reached while the game runs, but it reads its own files when the
//! player logs in or types /reload. So the companion writes `Glossary.lua` into the
//! addon's folder: the WoW terms of the slang and place lists, in both scripts, and the
//! lines seen at least twice with their English. Nothing touches the game itself.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Lines longer than this, in characters, are not kept: long lines are seldom said again.
const LONGEST: usize = 60;
/// How many lines are kept; past this the least used go first.
const MOST: usize = 5000;
/// A line goes to the game's chat once it has been seen this many times.
const SEEN_FOR_GAME: u32 = 2;
/// The addon file the glossary is written to, in the addon's folder.
pub const FILE: &str = "Glossary.lua";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub english: String,
    /// How many times the line has come, the first included.
    pub seen: u32,
    /// When it last came, in seconds since 1970.
    pub last: u64,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Glossary {
    lines: HashMap<String, Entry>,
}

/// The form a line is kept under: trimmed, with runs of spaces made one.
pub fn key(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Glossary {
    fn path() -> PathBuf {
        crate::config::path().with_file_name("glossary.json")
    }

    /// The kept glossary, or an empty one without a file.
    pub fn load() -> Glossary {
        std::fs::read_to_string(Self::path()).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(text) = serde_json::to_string(self) {
            let _ = std::fs::write(Self::path(), text);
        }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// The English of a line seen before, counting this sighting.
    pub fn recall(&mut self, text: &str, now: u64) -> Option<String> {
        let entry = self.lines.get_mut(&key(text))?;
        entry.seen += 1;
        entry.last = now;
        Some(entry.english.clone())
    }

    /// Keeps a line's English, unless the line is too long to be said again.
    pub fn remember(&mut self, text: &str, english: &str, now: u64) {
        let key = key(text);
        if key.is_empty() || key.chars().count() > LONGEST || english.trim().is_empty() {
            return;
        }
        let entry = self.lines.entry(key).or_insert(Entry { english: String::new(), seen: 0, last: now });
        entry.english = english.trim().to_string();
        entry.seen += 1;
        entry.last = now;
        if self.lines.len() > MOST {
            // The least used go first, and of those the longest unseen.
            let mut ranked: Vec<(String, u32, u64)> =
                self.lines.iter().map(|(key, entry)| (key.clone(), entry.seen, entry.last)).collect();
            ranked.sort_by_key(|(_, seen, last)| (*seen, *last));
            for (key, _, _) in ranked.into_iter().take(self.lines.len() - MOST) {
                self.lines.remove(&key);
            }
        }
    }

    /// The lines that go to the game, sorted so the file only changes when they do.
    pub fn for_game(&self) -> Vec<(&str, &str)> {
        let mut lines: Vec<(&str, &str)> = self
            .lines
            .iter()
            .filter(|(_, entry)| entry.seen >= SEEN_FOR_GAME)
            .map(|(key, entry)| (key.as_str(), entry.english.as_str()))
            .collect();
        lines.sort();
        lines
    }
}

/// A Lua string literal holding `text`. A "|" in English is made "/", since in a chat
/// line it would start an escape sequence.
fn lua_string(text: &str, english: bool) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '|' if english => out.push('/'),
            c if (c as u32) < 32 => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The addon's Glossary.lua: the WoW terms in Traditional and Simplified, and the lines
/// given. Read by Glossary.lua's users in the addon as `ns.GLOSSARY_TERMS` and
/// `ns.GLOSSARY_LINES`, both Chinese to English.
pub fn lua(lines: &[(&str, &str)]) -> String {
    let mut terms: Vec<(String, &str)> = Vec::new();
    for (zh, en) in crate::translate::terms() {
        for written in [zh.to_string(), crate::translate::to_simplified(zh), crate::translate::to_taiwanese(zh)] {
            if !terms.iter().any(|(known, _)| *known == written) {
                terms.push((written, en));
            }
        }
    }
    terms.sort();
    let mut out = String::from(
        "-- Written by the Nai Ma Speak companion; changes made here are lost when it writes it again.\n\
         -- WoW terms and chat lines with their English, shown in the game's chat when the\n\
         -- glossary is on. The companion adds lines it has translated at least twice.\n\
         local _, ns = ...\n\nns.GLOSSARY_TERMS = {\n",
    );
    for (zh, en) in &terms {
        out.push_str(&format!("    [{}] = {},\n", lua_string(zh, false), lua_string(en, true)));
    }
    out.push_str("}\n\nns.GLOSSARY_LINES = {\n");
    for (zh, en) in lines {
        out.push_str(&format!("    [{}] = {},\n", lua_string(zh, false), lua_string(en, true)));
    }
    out.push_str("}\n");
    out
}

/// The addon's folder: the one set in the settings, or else the one the companion runs
/// from if the addon is there, or else the first of the usual places that has the addon.
pub fn addon_folder(set: &str) -> Option<PathBuf> {
    let has_addon = |dir: &Path| dir.join("Translate.toc").is_file();
    if !set.trim().is_empty() {
        return Some(PathBuf::from(set.trim())).filter(|dir| has_addon(dir));
    }
    let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
    if let Some(dir) = beside.filter(|dir| has_addon(dir)) {
        return Some(dir);
    }
    let roots = if cfg!(windows) {
        vec![r"C:\Program Files (x86)\World of Warcraft", r"C:\Program Files\World of Warcraft"]
    } else {
        vec!["/Applications/World of Warcraft"]
    };
    let flavors = ["_forever_", "_retail_", "_classic_era_", "_classic_", "_anniversary_"];
    roots
        .iter()
        .flat_map(|root| flavors.iter().map(move |flavor| Path::new(root).join(flavor).join("Interface/AddOns/Translate")))
        .find(|dir| has_addon(dir))
}

/// The glossary as the translators share it, with whether it changed since last saved.
#[derive(Default)]
pub struct Shared {
    pub glossary: Glossary,
    pub changed: bool,
}

pub type Handle = std::sync::Arc<std::sync::Mutex<Shared>>;

pub fn shared() -> Handle {
    std::sync::Arc::new(std::sync::Mutex::new(Shared { glossary: Glossary::load(), changed: false }))
}

/// How often a changed glossary is saved and written to the addon.
const WRITE_EVERY: std::time::Duration = std::time::Duration::from_secs(20);

/// Saves the glossary and writes it to the addon's folder: once at the start, so the
/// addon has the current terms, then whenever it changed, every `WRITE_EVERY` at most.
/// Tells the companion window how many lines it holds and where they went.
pub fn spawn_writer(shared: Handle, config: std::sync::Arc<std::sync::Mutex<crate::config::Config>>, ui: crate::worker::UiSender) {
    std::thread::spawn(move || {
        let mut first = true;
        let mut told: Option<(usize, usize, Option<PathBuf>)> = None;
        loop {
            let set = config.lock().unwrap().addon_folder.clone();
            let folder = addon_folder(&set);
            let (count, for_game, text) = {
                let mut shared = shared.lock().unwrap();
                let changed = std::mem::take(&mut shared.changed);
                if changed {
                    shared.glossary.save();
                }
                let text = (changed || first).then(|| lua(&shared.glossary.for_game()));
                (shared.glossary.len(), shared.glossary.for_game().len(), text)
            };
            if let (Some(text), Some(folder)) = (text, &folder) {
                let path = folder.join(FILE);
                if !std::fs::read_to_string(&path).is_ok_and(|old| old == text) {
                    match std::fs::write(&path, text) {
                        Ok(()) => crate::diag::log(format!("glossary: wrote {for_game} lines to {}", path.display())),
                        Err(e) => crate::diag::log(format!("glossary: could not write {}: {e}", path.display())),
                    }
                }
            }
            first = false;
            let now = (count, for_game, folder.clone());
            if told.as_ref() != Some(&now) {
                ui.send(crate::worker::UiEvent::Glossary { lines: count, in_game: for_game, folder });
                told = Some(now);
            }
            std::thread::sleep(WRITE_EVERY);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_lines_and_counts_them() {
        let mut glossary = Glossary::default();
        assert_eq!(glossary.recall("缺坦", 1), None);
        glossary.remember(" 缺坦  來個 ", "Need a tank, come on", 1);
        assert!(glossary.for_game().is_empty(), "seen once");
        assert_eq!(glossary.recall("缺坦 來個", 2).as_deref(), Some("Need a tank, come on"));
        assert_eq!(glossary.for_game(), [("缺坦 來個", "Need a tank, come on")]);
        glossary.remember(&"長".repeat(LONGEST + 1), "long", 3);
        assert_eq!(glossary.len(), 1, "long lines are not kept");
    }

    #[test]
    fn forgets_the_least_used_first() {
        let mut glossary = Glossary::default();
        glossary.remember("常說", "said often", 1);
        glossary.recall("常說", 2);
        for i in 0..MOST {
            glossary.remember(&format!("行{i}"), "once", 3 + i as u64);
        }
        assert_eq!(glossary.len(), MOST);
        assert!(glossary.recall("常說", 9).is_some(), "the line seen twice stays");
        assert!(glossary.recall("行0", 9).is_none(), "the oldest of those seen once went");
    }

    #[test]
    fn writes_lua_the_addon_can_read() {
        let lua = lua(&[("他說\"好\"", "He said \"OK\" | fine")]);
        assert!(lua.contains("ns.GLOSSARY_TERMS = {"));
        // Terms come in both scripts.
        assert!(lua.contains("[\"奶妈\"] = ") && lua.contains("[\"奶媽\"] = "), "both scripts");
        assert!(lua.contains("[\"他說\\\"好\\\"\"] = \"He said \\\"OK\\\" / fine\","), "{lua}");
    }

    /// The addon ships a Glossary.lua with the terms and no lines, so it has them before
    /// the companion first runs. Rewrite it with `translate-companion --write-glossary
    /// ../addon/Translate/Glossary.lua` when the slang lists change.
    #[test]
    fn the_addon_ships_the_current_terms() {
        let shipped = include_str!("../../addon/Translate/Glossary.lua").replace("\r\n", "\n");
        assert!(shipped == lua(&[]), "addon/Translate/Glossary.lua is out of date: run translate-companion --write-glossary ../addon/Translate/Glossary.lua");
    }
}
