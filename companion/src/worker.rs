use crate::config::Config;
use crate::markup::{ChatLine, Span};
use crate::protocol::Handling;
use crate::translate::{translate, Task};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

pub const TEST_LINE: &str = "大家好，今天一起打副本嗎？";
/// The addon's "/tr test" lines end in this. They are translated afresh every time, so
/// the same test can be run again to watch the backend at work.
const TEST_MARK: &str = "(Translate test)";

pub enum Job {
    /// A Chinese line read from the addon's pixel strip, already in the overlay as `line`.
    Incoming { line: u64, text: String },
    /// Such a line, to explain word by word.
    Explain { line: u64, text: String },
    /// A player's name to put into English, for showing after the name.
    Name { name: String },
    /// A player's name to explain, in the overlay line `line`.
    ExplainName { line: u64, name: String },
    /// English the player typed in the companion, to send as Chinese.
    Reply { text: String },
    /// The Test model button: a fixed line, never cached.
    Test,
}

pub enum UiEvent {
    /// A chat line for the overlay. A line that awaits something shows as it is until
    /// `Translated` or `Explained` brings it, so lines keep the order they were said in.
    Line { id: u64, line: ChatLine, awaiting: Option<Awaiting> },
    Translated { id: u64, english: String },
    /// The line's body, explained word by word.
    Explained { id: u64, explanation: Vec<Span> },
    /// What an overlay line awaited was given up on.
    Failed { id: u64 },
    /// A player's name in English.
    Named { name: String, english: String },
    /// A reply in Chinese. It is copied for pasting unless it is too long for WoW's chat
    /// box, which would cut it off.
    ReplyReady { english: String, chinese: String, copied: bool },
    TestResult { result: Result<String, String>, seconds: f32 },
    /// The game's chat windows, in screen points, whenever the addon reports them.
    ChatWindows(Vec<crate::layout::ChatWindow>),
    /// The glossary's size: lines remembered, those given to the game, and the addon
    /// folder it is written to, if found.
    Glossary { lines: usize, in_game: usize, folder: Option<std::path::PathBuf> },
    /// Where the strip is on screen, in points, once found; None once lost.
    StripPlace(Option<crate::focus::Rect>),
    /// The chat window the addon copies to the overlay; empty for all chat.
    SourceWindow(String),
    /// A reply could not be translated.
    ReplyFailed { english: String, reason: String },
    /// A button click or a typed reply from the game.
    Command(crate::protocol::Command),
    /// Mouse wheel steps a chat window has had since login, up counted positive.
    Wheel { name: String, count: i64 },
    /// Pixel strip state: searching, or where it was found.
    Strip(String),
    /// Whether WoW has focus; None when the focused window cannot be read.
    WowView(crate::focus::WowView),
    Status(String),
}

/// What an overlay line is waiting for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Awaiting {
    Translation,
    Explanation,
}

impl Awaiting {
    pub fn noun(self) -> &'static str {
        match self {
            Awaiting::Translation => "translation",
            Awaiting::Explanation => "explanation",
        }
    }
}

/// Sends to the UI and wakes it so the event shows without mouse movement.
#[derive(Clone)]
pub struct UiSender {
    tx: Sender<UiEvent>,
    ctx: eframe::egui::Context,
}

impl UiSender {
    pub fn new(tx: Sender<UiEvent>, ctx: eframe::egui::Context) -> Self {
        Self { tx, ctx }
    }

    pub fn send(&self, event: UiEvent) {
        let _ = self.tx.send(event);
        self.ctx.request_repaint();
    }
}

/// The color of the mark on a line the player asked for.
const ASKED_COLOR: crate::markup::Rgb = [0x66, 0xcc, 0xff];

/// The overlay's line for a chat line from the addon, numbered `id`, and the job that
/// translates or explains it, if the addon asked for that and there is Chinese to work
/// on. The addon does not ask for lines in which only a speaker's name is Chinese. The
/// job gets the body without its color codes. A line the player asked for is marked.
/// The speaker's name is remembered, for telling the translator about it later.
pub fn chat_line(id: u64, mut line: ChatLine, handling: Handling) -> (UiEvent, Option<Job>) {
    crate::names::remember(&line.speaker);
    let mark = |text: &str| Span { text: text.into(), color: Some(ASKED_COLOR) };
    if handling == Handling::ExplainName {
        // The line is the name and, once it arrives, what it means.
        let name = line.speaker.clone();
        line.prefix = vec![mark("[Name] "), Span::plain(format!("{name}: "))];
        line.body = Vec::new();
        let job = Job::ExplainName { line: id, name };
        return (UiEvent::Line { id, line, awaiting: Some(Awaiting::Explanation) }, Some(job));
    }
    match handling {
        Handling::TranslateFirst => line.prefix.insert(0, mark("[On request] ")),
        Handling::Explain => line.prefix.insert(0, mark("[Explain] ")),
        _ => {}
    }
    let text = crate::markup::plain(&line.body).trim().to_string();
    if handling == Handling::Show || !crate::text::has_chinese(&text) {
        return (UiEvent::Line { id, line, awaiting: None }, None);
    }
    let (awaiting, job) = match handling {
        Handling::Explain => (Awaiting::Explanation, Job::Explain { line: id, text }),
        _ => (Awaiting::Translation, Job::Incoming { line: id, text }),
    };
    (UiEvent::Line { id, line, awaiting: Some(awaiting) }, Some(job))
}

/// One thread per queue keeps the chat log from delaying click-to-translate.
pub fn spawn(
    config: Arc<Mutex<Config>>,
    jobs: Receiver<Job>,
    ui: UiSender,
    clipboard: Sender<String>,
    glossary: crate::glossary::Handle,
) {
    thread::spawn(move || {
        let mut cache: HashMap<String, String> = HashMap::new();
        for job in jobs {
            let config = config.lock().unwrap().clone();
            // Notes from the backend, such as a wait; cleared once the line gets through.
            let noted = std::cell::Cell::new(false);
            let notify = |note: String| {
                noted.set(true);
                ui.send(UiEvent::Status(note));
            };
            if let Job::Test = job {
                let started = Instant::now();
                let result = translate(&config, TEST_LINE, Task::ToEnglish, &notify);
                ui.send(UiEvent::TestResult { result, seconds: started.elapsed().as_secs_f32() });
                continue;
            }
            let (text, task) = match &job {
                Job::Incoming { text, .. } => (text.clone(), Task::ToEnglish),
                Job::Explain { text, .. } => (text.clone(), Task::Explain(config.reading)),
                Job::Name { name } => (name.clone(), Task::Name),
                Job::ExplainName { name, .. } => (name.clone(), Task::ExplainName),
                Job::Reply { text } => (text.clone(), Task::ToChinese),
                Job::Test => unreachable!("handled above"),
            };
            let key = format!("{:?}|{:?}|{task:?}|{text}", config.backend, (config.server, config.reply_variant));
            // A chat line said before is answered from the glossary first, and a new one
            // goes into it once translated. Test lines are translated afresh every time.
            let in_glossary = matches!(job, Job::Incoming { .. }) && config.remember_translations && !text.contains(TEST_MARK);
            let now = crate::backlog::now();
            let remembered = if in_glossary {
                let mut shared = glossary.lock().unwrap();
                let english = shared.glossary.recall(&text, now);
                shared.changed |= english.is_some();
                english
            } else {
                None
            };
            let from_memory = remembered.is_some();
            let result = match remembered.or_else(|| cache.get(&key).filter(|_| !text.contains(TEST_MARK)).cloned()) {
                Some(hit) => Ok(hit),
                None => translate(&config, &text, task, &notify),
            };
            if let (true, false, Ok(english)) = (in_glossary, from_memory, &result) {
                let mut shared = glossary.lock().unwrap();
                shared.glossary.remember(&text, english, now);
                shared.changed = true;
            }
            // An explanation that cannot be read counts as a failure, and is not kept.
            let explanation = match (&job, &result) {
                (Job::Explain { .. }, Ok(answer)) => Some(crate::explain::parse(answer)),
                _ => None,
            };
            let result = match &explanation {
                Some(Err(problem)) => Err(problem.clone()),
                _ => result,
            };
            let translated = match result {
                Ok(t) => {
                    if noted.get() {
                        ui.send(UiEvent::Status("Ready".into()));
                    }
                    if cache.len() > 2000 {
                        cache.clear();
                    }
                    cache.insert(key, t.clone());
                    t
                }
                Err(e) => {
                    ui.send(UiEvent::Status(format!("Translation failed: {e}")));
                    match job {
                        Job::Incoming { line, .. } | Job::Explain { line, .. } | Job::ExplainName { line, .. } => {
                            ui.send(UiEvent::Failed { id: line })
                        }
                        Job::Reply { text } => ui.send(UiEvent::ReplyFailed { english: text, reason: e }),
                        _ => {}
                    }
                    continue;
                }
            };
            match job {
                Job::Incoming { line, .. } => ui.send(UiEvent::Translated { id: line, english: translated }),
                Job::Explain { line, .. } => {
                    if let Some(Ok(explanation)) = &explanation {
                        ui.send(UiEvent::Explained { id: line, explanation: crate::explain::spans(explanation) });
                    }
                }
                Job::Name { name } => ui.send(UiEvent::Named { name, english: translated }),
                Job::ExplainName { line, .. } => {
                    ui.send(UiEvent::Explained { id: line, explanation: vec![Span::plain(translated)] })
                }
                Job::Reply { text } => {
                    let copied = translated.len() <= crate::translate::WOW_CHAT_BYTES;
                    if copied {
                        let _ = clipboard.send(translated.clone());
                    }
                    ui.send(UiEvent::ReplyReady { english: text, chinese: translated, copied });
                }
                Job::Test => {}
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{parse, Message};

    fn chat(line: &str) -> (UiEvent, Option<Job>) {
        match parse(line) {
            Some(Message::Chat { line, handling }) => chat_line(7, line, handling),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn only_lines_the_addon_marks_go_to_the_translator() {
        let (UiEvent::Line { id: 7, awaiting: Some(Awaiting::Translation), .. }, Some(Job::Incoming { line: 7, text })) =
            chat("1\t[Say] [Lili]: \t副本缺T |cffa335ee[劍]|r \t40ff40")
        else {
            panic!("the Chinese line shows untranslated and is queued");
        };
        assert_eq!(text, "副本缺T [劍]");
        // A copied line with only a Chinese name is shown as it is.
        assert!(matches!(chat("0\t\t[小明] says: hello"), (UiEvent::Line { awaiting: None, .. }, None)));
        // So is a marked line that turns out to have no Chinese.
        assert!(matches!(chat("1\t\thello"), (UiEvent::Line { awaiting: None, .. }, None)));
    }

    #[test]
    fn a_line_asked_for_again_is_marked() {
        let (UiEvent::Line { line, awaiting: Some(Awaiting::Translation), .. }, Some(Job::Incoming { .. })) =
            chat("T\t[Say] [Lili]: \t缺坦\tffffff")
        else {
            panic!("a line asked for is translated");
        };
        assert_eq!(crate::markup::plain(&line.prefix), "[On request] [Say] [Lili]: ");
        let (UiEvent::Line { line, awaiting: Some(Awaiting::Explanation), .. }, Some(Job::Explain { text, .. })) =
            chat("E\t[Say] [Lili]: \t缺坦\tffffff")
        else {
            panic!("a line to explain is explained");
        };
        assert_eq!((crate::markup::plain(&line.prefix).as_str(), text.as_str()), ("[Explain] [Say] [Lili]: ", "缺坦"));
        let (UiEvent::Line { line, awaiting: Some(Awaiting::Explanation), .. }, Some(Job::ExplainName { name, .. })) =
            chat("N\t\t\tffffff\t工作測試名")
        else {
            panic!("a name to explain is explained");
        };
        assert_eq!((crate::markup::plain(&line.prefix).as_str(), name.as_str()), ("[Name] 工作測試名: ", "工作測試名"));
        // A speaker is remembered, and pointed out when a later line names them.
        assert_eq!(crate::names::in_line("跟工作測試名走"), ["工作測試名"]);
    }
}
