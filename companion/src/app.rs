use crate::backends::claude::key_problem;
use crate::cover;
use crate::config::{self, Backend, Config, Reading, Region, CLAUDE_MODELS, CUSTOM_LABEL, ENDPOINT_PRESETS};
use crate::translate::WOW_CHAT_BYTES;
use crate::focus::WowView;
use crate::layout::{self, ChatWindow};
use crate::protocol::Command;
use crate::backlog;
use crate::markup::{self, ChatLine, Span};
use crate::theme;
use crate::worker::{Awaiting, Job, UiEvent};
use eframe::egui::text::LayoutJob;
use eframe::egui::{self, Color32, RichText, TextFormat, ViewportBuilder, ViewportCommand, ViewportId};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
const HIDE_LABEL: &str = "Hide when WoW is hidden or covered";
#[cfg(not(target_os = "macos"))]
const HIDE_LABEL: &str = "Hide when WoW is in the background";

/// How often the whole font texture is re-sent even without new text.
const FONT_TEXTURE_REFRESH: Duration = Duration::from_secs(5);

/// How many lines one mouse wheel step scrolls the overlay.
const WHEEL_LINES: f32 = 3.0;
/// How far the overlay text's shadow is offset, in points.
const SHADOW: f32 = 1.0;
/// How much of its brightness a Chinese line keeps while it waits for its translation.
const PENDING_DIM: f32 = 0.6;

struct OverlayLine {
    /// The number `UiEvent::Translated` refers to; 0 for a line brought back from the
    /// last run, which nothing refers to any more.
    id: u64,
    /// When the line arrived, in seconds since 1970.
    seen: u64,
    line: ChatLine,
    translation: Translation,
}

/// Where the translation or explanation of a line's body stands.
enum Translation {
    /// The body is shown as it is.
    NotNeeded,
    Pending(Awaiting),
    /// Given up on; the body stays in Chinese.
    Failed(Awaiting),
    Done(Vec<Span>),
}

impl OverlayLine {
    /// A line as it is written to the backlog file. One still waiting is kept without
    /// its translation.
    fn kept(&self) -> backlog::Kept {
        let done = match &self.translation {
            Translation::Done(spans) => Some(spans.clone()),
            _ => None,
        };
        backlog::Kept { seen: self.seen, line: self.line.clone(), done }
    }

    /// A line of the last run, back in the overlay. What it was still waiting for then
    /// will not come now, so it is shown as it is; its [T] link asks again.
    fn restored(kept: backlog::Kept) -> Self {
        let translation = kept.done.map_or(Translation::NotNeeded, Translation::Done);
        Self { id: 0, seen: kept.seen, line: kept.line, translation }
    }

    /// The line as the game words and colors it, with the body in English when there is
    /// one. With `names`, the speaker's name is followed by what it is in English, once
    /// that is known.
    fn job(&self, font: &egui::FontId, width: f32, names: Option<&HashMap<String, Option<String>>>) -> LayoutJob {
        let mut job = LayoutJob::default();
        job.wrap.max_width = width;
        let (body, dim, note) = match &self.translation {
            Translation::NotNeeded => (&self.line.body, 1.0, String::new()),
            Translation::Pending(awaiting) => (&self.line.body, PENDING_DIM, format!(" ({} pending)", awaiting.noun())),
            Translation::Failed(awaiting) => (&self.line.body, 1.0, format!(" ({} failed)", awaiting.noun())),
            Translation::Done(english) => (english, 1.0, String::new()),
        };
        let aside = Color32::from_gray(150);
        let speaker = self.line.speaker.as_str();
        let mut english = names.and_then(|names| names.get(speaker)).and_then(|english| english.as_deref());
        let parts = self.line.prefix.iter().map(|span| (span, 1.0)).chain(body.iter().map(|span| (span, dim)));
        for (span, brightness) in parts {
            let [r, g, b] = span.color.unwrap_or(self.line.color);
            let color = Color32::from_rgb(r, g, b).gamma_multiply(brightness);
            // The first time the speaker's name comes up, its English goes right after it.
            let name_ends = english.and_then(|_| span.text.find(speaker)).map(|at| at + speaker.len());
            let (before, after) = span.text.split_at(name_ends.unwrap_or(span.text.len()));
            job.append(before, 0.0, TextFormat::simple(font.clone(), color));
            if let Some(english) = english.filter(|_| name_ends.is_some()) {
                job.append(&format!(" ({english})"), 0.0, TextFormat::simple(font.clone(), aside));
            }
            if name_ends.is_some() {
                english = None;
            }
            job.append(after, 0.0, TextFormat::simple(font.clone(), color));
        }
        job.append(&note, 0.0, TextFormat::simple(font.clone(), aside));
        job
    }
}

pub struct TranslateApp {
    config: Arc<Mutex<Config>>,
    draft: Config,
    events: Receiver<UiEvent>,
    interactive: Sender<Job>,
    lines: VecDeque<OverlayLine>,
    status: String,
    notice: String,
    reply: String,
    locked: bool,
    overlay_start: egui::Pos2,
    overlay_rect: Option<egui::Rect>,
    test_result: String,
    /// The report on the Status tab was copied.
    report_copied: bool,
    strip_status: String,
    passes: crate::diag::Passes,
    /// The overlay's lines changed since the backlog file was written, and when it was.
    backlog_changed: bool,
    backlog_written: Instant,
    /// Chinese player names and what they are in English; `None` once asked for and
    /// until answered, or when there was nothing to add.
    names: HashMap<String, Option<String>>,
    /// The tab the overlay was anchored to before anchoring was turned off.
    last_anchor: Option<String>,
    /// The settings as last written to the file, and as they were when the companion
    /// started, which Undo goes back to.
    saved: Config,
    opened: Config,
    /// When the settings last differed from the file's.
    changed_at: Option<Instant>,
    undo_asked: bool,
    /// The tab being pictured and since when; see `shots`.
    shot: Option<(usize, Instant)>,
    wow_view: WowView,
    /// When the whole font texture was last sent; see `refresh_font_texture`.
    font_texture_sent: Instant,
    /// New overlay text arrived, so the font texture is re-sent after this frame.
    font_texture_stale: bool,
    chat_windows: Vec<ChatWindow>,
    /// The chat window the addon copies to the overlay, empty for all chat; None until an
    /// addon that reports it has said.
    source_window: Option<String>,
    /// Where the strip is on screen, in points, while it is found.
    strip_place: Option<crate::focus::Rect>,
    /// Whether the strip cover window has been left out of screen captures; until then it
    /// is kept off the strip.
    cover_hidden_from_capture: bool,
    /// Where the strip cover was last sent.
    cover_sent: Option<egui::Pos2>,
    /// Whether the overlay was on a chat tab last frame, and where it was last sent.
    overlay_anchored: bool,
    overlay_sent: Option<egui::Pos2>,
    overlay_hidden: bool,
    /// The settings show the fields for typing an endpoint, or a Claude model, by hand.
    custom_endpoint: bool,
    custom_claude_model: bool,
    /// The last mouse wheel count the addon reported for each chat window, and the steps
    /// over the overlay's tab that the overlay has not scrolled by yet.
    wheel_counts: HashMap<String, i64>,
    wheel_steps: f32,
    tab: Tab,
    /// Where the last reply stands; shown as a pill on the overlay.
    reply_state: ReplyState,
    /// The reply box is open, and whether it still has to take the keyboard.
    reply_box: bool,
    reply_box_focus: bool,
    /// How tall the reply box's contents were when last drawn; the box is made that tall.
    reply_box_height: f32,
    /// The game's App button asked for this window to come forward.
    show_window: bool,
    /// This window was hidden with its Hide button.
    window_hidden: bool,
    /// The window has been on screen, so having no rectangle now means it is minimized.
    window_placed: bool,
    /// The game's App button brought this window over the game, where it still is.
    window_summoned: bool,
    /// Whether the companion was the app in front at the last look.
    was_active: bool,
}

/// Where a reply typed in English stands.
enum ReplyState {
    Idle,
    Translating,
    /// Copied for pasting; goes back to idle a little later.
    Copied(Instant),
    /// Not translated, and why.
    Failed(String),
    /// Translated into this, but it is longer than one chat message, so it was not copied.
    TooLong(String),
}

/// How tall the companion window's own title bar is: its buttons and the space around them.
const TITLE_BAR_HEIGHT: f32 = 46.0;
/// Where a new line goes among the overlay's lines, given their numbers: at the end,
/// or, for a line of the chat that arrives late (the addon sends its last lines again
/// for a companion that missed them), before the lines that came after it in the game.
fn place(lines: impl ExactSizeIterator<Item = Option<u64>>, new: Option<u64>) -> usize {
    let count = lines.len();
    let Some(new) = new else { return count };
    lines.into_iter().position(|number| number.is_some_and(|number| number > new)).unwrap_or(count)
}

/// The backlog file is written at most this often.
const BACKLOG_EVERY: Duration = Duration::from_secs(2);
/// How long settings are left alone before they are written to the file.
const SAVE_AFTER: Duration = Duration::from_millis(500);
/// How long the overlay says a reply was copied.
const COPIED_SHOWN: Duration = Duration::from_secs(6);
/// How far above the chat window the reply box opens, past the tab row.
const REPLY_BOX_TITLE: &str = "Translate reply";
const REPLY_BOX_GAP: f32 = 34.0;

/// Draws a window of its own, as `show_viewport_immediate` does, and says whether it did.
/// eframe (0.36 and 0.37) cannot open a new window in a pass it starts from a timer while
/// the main window is hidden: it skips the window without a word, and egui panics because
/// the window's contents were never drawn. That one panic is caught here and the window
/// opens in the next pass, which is asked for in a way that can open windows. The fix is
/// on egui's main branch (`new_events` in eframe's run.rs); this can go once it is released.
fn show_window(ctx: &egui::Context, id: ViewportId, builder: ViewportBuilder, ui: impl FnMut(&mut egui::Ui, egui::ViewportClass)) -> bool {
    let drawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ctx.show_viewport_immediate(id, builder, ui)));
    match drawn {
        Ok(()) => true,
        Err(panic) if crate::diag::is_unopened_window(panic_message(&*panic)) => {
            ctx.request_repaint();
            false
        }
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

/// What a caught panic said, if it said it in words.
fn panic_message(panic: &(dyn std::any::Any + Send)) -> &str {
    let message = panic.downcast_ref::<String>().map(String::as_str);
    message.or(panic.downcast_ref::<&str>().copied()).unwrap_or("")
}

fn reply_id() -> ViewportId {
    ViewportId::from_hash_of("reply")
}

/// The sections of the companion window.
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Backend,
    Overlay,
    Status,
    History,
    /// Typing a reply in the companion, for use outside the game.
    Translate,
}

impl Tab {
    /// Typing a reply here comes last: in the game its own Translate button does that.
    const ALL: [(Tab, &'static str); 5] = [
        (Tab::Backend, "Backend"),
        (Tab::Overlay, "Overlay"),
        (Tab::Status, "Status"),
        (Tab::History, "Chat history"),
        (Tab::Translate, "Translate"),
    ];
}

/// Smallest companion window that shows every control with Settings closed.
pub const MIN_WIDTH: f32 = 560.0;
pub const MIN_HEIGHT: f32 = 320.0;
/// The size the companion window opens at: room for the tallest tab.
pub const START_SIZE: [f32; 2] = [620.0, 560.0];

const OFF_SCREEN: f32 = -32000.0;
/// Whether a hidden overlay is moved off screen; see `overlay`.
const PARK_HIDDEN_OVERLAY: bool = !cfg!(target_os = "macos");

/// A drop-down of ready-made choices and, last, "Custom". Returns what was clicked, if
/// anything: the index of a ready-made choice, or `None` for Custom.
fn preset_combo(ui: &mut egui::Ui, id: &str, labels: &[&str], chosen: Option<usize>) -> Option<Option<usize>> {
    let mut picked = None;
    let shown = chosen.map_or(CUSTOM_LABEL, |index| labels[index]);
    egui::ComboBox::from_id_salt(id).width(ui.available_width()).selected_text(shown).show_ui(ui, |ui| {
        for (index, label) in labels.iter().enumerate() {
            if ui.selectable_label(chosen == Some(index), *label).clicked() {
                picked = Some(Some(index));
            }
        }
        if ui.selectable_label(chosen.is_none(), CUSTOM_LABEL).clicked() {
            picked = Some(None);
        }
    });
    picked
}

/// A setting that is a number: its name, then a slider.
fn slider<N: egui::emath::Numeric>(ui: &mut egui::Ui, name: &str, value: &mut N, range: std::ops::RangeInclusive<N>) {
    ui.label(theme::label(name));
    ui.add(egui::Slider::new(value, range));
}

/// The command that has the addon copy the chat window `name` to the overlay, or all chat
/// for an empty name.
fn source_command(name: &str) -> String {
    format!("/tr /window {}", if name.is_empty() { "all" } else { name })
}

fn cover_id() -> ViewportId {
    ViewportId::from_hash_of("strip cover")
}

/// The strip cover's color: near black, like a dark corner of the game.
const COVER_COLOR: Color32 = Color32::from_rgb(10, 10, 12);

fn overlay_id() -> ViewportId {
    ViewportId::from_hash_of("overlay")
}

impl TranslateApp {
    pub fn new(config: Arc<Mutex<Config>>, events: Receiver<UiEvent>, interactive: Sender<Job>) -> Self {
        let draft = config.lock().unwrap().clone();
        let overlay_start = egui::pos2(draft.overlay_x, draft.overlay_y);
        Self {
            config,
            events,
            interactive,
            lines: backlog::load().into_iter().map(OverlayLine::restored).collect(),
            status: "Ready".into(),
            notice: String::new(),
            reply: String::new(),
            locked: true,
            overlay_start,
            overlay_rect: None,
            passes: crate::diag::Passes::new(),
            backlog_changed: false,
            backlog_written: Instant::now(),
            names: HashMap::new(),
            last_anchor: None,
            saved: draft.clone(),
            opened: draft.clone(),
            changed_at: None,
            undo_asked: false,
            shot: None,
            wow_view: WowView::Unknown,
            font_texture_sent: Instant::now(),
            font_texture_stale: false,
            chat_windows: Vec::new(),
            source_window: None,
            strip_place: None,
            cover_hidden_from_capture: false,
            cover_sent: None,
            overlay_anchored: false,
            overlay_sent: None,
            overlay_hidden: false,
            custom_endpoint: draft.endpoint_preset().is_none(),
            custom_claude_model: draft.claude_preset().is_none(),
            wheel_counts: HashMap::new(),
            wheel_steps: 0.0,
            tab: Tab::Backend,
            reply_state: ReplyState::Idle,
            reply_box: false,
            reply_box_focus: false,
            reply_box_height: 56.0,
            show_window: false,
            window_hidden: false,
            window_placed: false,
            window_summoned: false,
            was_active: true,
            test_result: String::new(),
            report_copied: false,
            strip_status: "Starting the pixel strip reader".into(),
            draft,
        }
    }

    fn push_line(&mut self, mut line: OverlayLine) {
        let number = line.line.number;
        // The same number again is the line worded anew, with a link now named in
        // English: it takes the earlier one's place, and keeps showing its translation
        // until its own arrives.
        if let Some(earlier) = self.lines.iter_mut().find(|other| number.is_some() && other.line.number == number) {
            if let (Translation::Pending(_), Translation::Done(_)) = (&line.translation, &earlier.translation) {
                line.translation = std::mem::replace(&mut earlier.translation, Translation::NotNeeded);
            }
            line.seen = earlier.seen;
            *earlier = line;
        } else {
            let place = place(self.lines.iter().map(|line| line.line.number), number);
            self.lines.insert(place, line);
        }
        while self.lines.len() > self.draft.max_lines.max(1) {
            self.lines.pop_front();
        }
        self.font_texture_stale = true;
        self.backlog_changed = true;
    }

    /// Writes the overlay's lines to the backlog file a little after they change, so a
    /// burst of chat is one write.
    fn keep_backlog(&mut self, ctx: &egui::Context) {
        if !self.backlog_changed {
            return;
        }
        match BACKLOG_EVERY.checked_sub(self.backlog_written.elapsed()) {
            Some(left) => ctx.request_repaint_after(left),
            None => self.write_backlog(),
        }
    }

    fn write_backlog(&mut self) {
        let kept: Vec<backlog::Kept> = self.lines.iter().map(OverlayLine::kept).collect();
        backlog::save(&kept);
        self.backlog_changed = false;
        self.backlog_written = Instant::now();
    }

    /// Sends the whole font atlas to the GPU again, after new overlay text and
    /// every few seconds anyway.
    ///
    /// egui normally uploads only the glyphs added since the last frame. On
    /// macOS that stopped working once after some hours: every glyph
    /// rasterized from then on drew as fragments of other glyphs in both
    /// windows until the app was restarted. Atlas growth, light/dark theme
    /// switches, window resizes and overlay hiding did not reproduce it, so
    /// rather than guess at the trigger the companion re-sends the complete
    /// atlas, which repairs the texture whatever went wrong. The copy is a few
    /// megabytes, so it is cheap at this rate.
    fn refresh_font_texture(&mut self, ctx: &egui::Context) {
        let due = self.font_texture_stale || self.font_texture_sent.elapsed() >= FONT_TEXTURE_REFRESH;
        if !due {
            return;
        }
        let image = ctx.fonts(|fonts| fonts.image());
        let delta = egui::epaint::ImageDelta::full(image, egui::epaint::TextureAtlas::texture_options());
        ctx.tex_manager().write().set(egui::TextureId::default(), delta);
        if self.font_texture_stale {
            // The overlay was painted before this upload; show it again with the repaired texture.
            ctx.request_repaint();
        }
        self.font_texture_sent = Instant::now();
        self.font_texture_stale = false;
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                UiEvent::Line { id, line, awaiting } => {
                    let translation = awaiting.map_or(Translation::NotNeeded, Translation::Pending);
                    self.push_line(OverlayLine { id, seen: backlog::now(), line, translation });
                }
                UiEvent::Translated { id, english } => {
                    if let Some(line) = self.lines.iter_mut().rev().find(|line| line.id == id) {
                        line.translation = Translation::Done(markup::carry_colors(&english, &line.line.body));
                        self.font_texture_stale = true;
                        self.backlog_changed = true;
                    }
                }
                UiEvent::Explained { id, explanation } => {
                    if let Some(line) = self.lines.iter_mut().rev().find(|line| line.id == id) {
                        line.translation = Translation::Done(explanation);
                        self.font_texture_stale = true;
                        self.backlog_changed = true;
                    }
                }
                UiEvent::Named { name, english } => {
                    // A name that came back as it went has nothing to add.
                    let english = Some(english).filter(|english| !english.is_empty() && *english != name);
                    self.names.insert(name, english);
                    self.font_texture_stale = true;
                }
                UiEvent::Failed { id } => {
                    if let Some(line) = self.lines.iter_mut().rev().find(|line| line.id == id) {
                        if let Translation::Pending(awaiting) = line.translation {
                            line.translation = Translation::Failed(awaiting);
                        }
                    }
                }
                UiEvent::ReplyReady { english, chinese, copied } => {
                    self.notice = if copied {
                        format!("Copied \"{chinese}\" for \"{english}\". Paste it into WoW chat.")
                    } else {
                        format!("Not copied: \"{chinese}\" is too long for WoW chat. Say it in fewer words.")
                    };
                    self.reply_state = if copied { ReplyState::Copied(Instant::now()) } else { ReplyState::TooLong(chinese) };
                }
                UiEvent::ReplyFailed { english, reason } => {
                    // The English goes back into the box, for another try when it is next opened.
                    self.reply = english;
                    self.reply_state = ReplyState::Failed(reason);
                }
                UiEvent::Command(Command::OpenReply) => {
                    // The game's button opens the box, and closes it when it is open.
                    self.reply_box = !self.reply_box;
                    self.reply_box_focus = self.reply_box;
                }
                UiEvent::Command(Command::OpenSettings) => self.show_window = true,
                UiEvent::Command(Command::Reply(english)) => self.send_reply(english),
                UiEvent::TestResult { result, seconds } => {
                    let backend = self.config.lock().unwrap().backend.label();
                    self.test_result = match result {
                        Ok(english) if crate::text::has_chinese(&english) => {
                            format!("{backend} answered but did not translate ({seconds:.1} s): {english}")
                        }
                        Ok(english) => {
                            format!("{backend} works ({seconds:.1} s): {} -> {english}", crate::worker::TEST_LINE)
                        }
                        Err(e) => format!("{backend} test failed: {e}"),
                    };
                }
                UiEvent::Strip(text) => self.strip_status = text,
                UiEvent::WowView(view) => self.wow_view = view,
                UiEvent::ChatWindows(windows) => {
                    // Once, and when tabs come or go: not each time one is selected or moved.
                    let names = |windows: &[layout::ChatWindow]| windows.iter().map(|window| window.name.clone()).collect::<Vec<_>>();
                    if names(&windows) != names(&self.chat_windows) {
                        crate::diag::log(format!("chat windows: {windows:?}"));
                    }
                    self.chat_windows = windows;
                }
                UiEvent::StripPlace(place) => self.strip_place = place,
                UiEvent::SourceWindow(name) => {
                    if self.source_window.as_ref() != Some(&name) {
                        crate::diag::log(format!("source window: {:?}", if name.is_empty() { "all chat" } else { &name }));
                    }
                    self.source_window = Some(name);
                }
                UiEvent::Wheel { name, count } => {
                    // The first count only sets the starting point: its steps were made
                    // before the companion was watching.
                    let before = self.wheel_counts.insert(name.clone(), count);
                    let anchored = layout::anchor(&self.chat_windows, &self.draft.anchor_tab);
                    if let (Some(before), Some(window)) = (before, anchored) {
                        if window.name == name {
                            self.wheel_steps += (count - before) as f32;
                        }
                    }
                }
                UiEvent::Status(text) => {
                    if text != self.status {
                        crate::diag::log(format!("status: {text}"));
                    }
                    self.status = text;
                }
            }
        }
    }

    /// For looking at the window without sitting at the screen: with TRANSLATE_SHOTS set
    /// to a folder, saves a picture of this window on each tab and of the reply box there, then quits.
    fn shots(&mut self, ctx: &egui::Context) {
        let Some(folder) = std::env::var_os("TRANSLATE_SHOTS") else {
            return;
        };
        ctx.request_repaint_after(Duration::from_millis(100));
        if self.shot.is_none() {
            // Above other windows, or a covered window is not drawn and pictures go stale.
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
            // On the sharpest display there is, for pictures worth showing.
            let sharpest = xcap::Monitor::all().unwrap_or_default().into_iter().max_by(|a, b| {
                a.scale_factor().unwrap_or(1.0).total_cmp(&b.scale_factor().unwrap_or(1.0))
            });
            if let Some((Ok(x), Ok(y))) = sharpest.map(|monitor| (monitor.x(), monitor.y())) {
                let corner = egui::pos2(x as f32 + 120.0, y as f32 + 120.0);
                ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::OuterPosition(corner));
            }
        }
        let (step, since) = self.shot.get_or_insert((0, Instant::now()));
        if since.elapsed() < Duration::from_millis(900) {
            return;
        }
        // The tabs of the main window one by one, then the reply box.
        let (title, name) = match Tab::ALL.get(*step) {
            Some(_) => ("Speak", format!("tab-{step}")),
            None => (REPLY_BOX_TITLE, "reply-box".to_string()),
        };
        let own = xcap::Window::all().unwrap_or_default().into_iter().find(|window| {
            window.pid().ok() == Some(std::process::id()) && window.title().is_ok_and(|found| found.contains(title))
        });
        match own.map(|window| window.capture_image()) {
            Some(Ok(image)) => {
                let _ = image.save(std::path::Path::new(&folder).join(format!("{name}.png")));
            }
            Some(Err(e)) => crate::diag::log(format!("no picture of {name}: {e}")),
            None => crate::diag::log(format!("no picture of {name}: window not found")),
        }
        *step += 1;
        *since = Instant::now();
        match Tab::ALL.get(*step) {
            Some((tab, _)) => self.tab = *tab,
            None if !self.reply_box => self.reply_box = true,
            None => {
                // The report of the Status tab's button too, to read without clicking it.
                let report = crate::diag::report(&self.state_for_report());
                let _ = std::fs::write(std::path::Path::new(&folder).join("report.txt"), report);
                std::process::exit(0)
            }
        }
    }

    /// Whether player names are followed by their English: asked for in the settings, and
    /// the backend can do it.
    fn names_shown(&self) -> bool {
        self.draft.translate_names && self.draft.backend.takes_instructions()
    }

    /// With names translated, asks for each Chinese speaker's name once.
    fn ask_for_names(&mut self) {
        if !self.names_shown() {
            return;
        }
        for line in &self.lines {
            let name = &line.line.speaker;
            if crate::text::has_chinese(name) && !self.names.contains_key(name) {
                self.names.insert(name.clone(), None);
                let _ = self.interactive.send(Job::Name { name: name.clone() });
            }
        }
    }

    fn send_reply(&mut self, english: String) {
        let _ = self.interactive.send(Job::Reply { text: english });
        self.reply_state = ReplyState::Translating;
    }

    /// What went wrong with the last reply, if something did, and the color to say it in.
    /// WoW's chat box holds 255 bytes, and a Chinese character takes 3 of them.
    fn reply_problem(&self) -> Option<(String, Color32)> {
        match &self.reply_state {
            ReplyState::Failed(reason) => Some((format!("Not translated: {reason} Enter tries again."), theme::BAD)),
            ReplyState::TooLong(chinese) => {
                let bytes = chinese.len();
                Some((format!("Not copied: {bytes} of {WOW_CHAT_BYTES} bytes. Say it in fewer words."), theme::GOLD))
            }
            _ => None,
        }
    }

    /// Copies a reply that is too long for one chat message all the same.
    fn copy_anyway(&mut self, ctx: &egui::Context) {
        if let ReplyState::TooLong(chinese) = &self.reply_state {
            ctx.copy_text(chinese.clone());
            self.reply_state = ReplyState::Copied(Instant::now());
            self.notice = "Copied the long reply. WoW will cut it at 255 bytes.".into();
        }
    }

    /// The small window for typing a reply, opened by the game's Translate button above the
    /// chat window. Enter sends the text off and closes it at once, so it never sits over
    /// the game while the translation is on its way; the overlay's pill shows that.
    fn reply_box(&mut self, ctx: &egui::Context) {
        if !self.reply_box {
            return;
        }
        let window = layout::anchor(&self.chat_windows, &self.draft.anchor_tab).map(|window| window.rect);
        let area = window.or(self.overlay_rect.map(|r| [r.min.x, r.min.y, r.width(), r.height()]));
        let width = area.map_or(520.0, |area| area[2].clamp(380.0, 720.0));
        let mut builder = ViewportBuilder::default()
            .with_title(REPLY_BOX_TITLE)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_active(true)
            .with_inner_size([width, self.reply_box_height]);
        if let Some(area) = area {
            builder = builder.with_position([area[0], area[1] - self.reply_box_height - REPLY_BOX_GAP]);
        }
        let problem = self.reply_problem();
        let edge = problem.as_ref().map_or(theme::BORDER, |(_, color)| *color);
        let too_long = matches!(self.reply_state, ReplyState::TooLong(_));
        let take_focus = std::mem::take(&mut self.reply_box_focus);
        let (mut submit, mut close, mut copy_anyway) = (false, false, false);
        let mut height = self.reply_box_height;
        let reply = &mut self.reply;
        let shown = show_window(ctx, reply_id(), builder, |root, _class| {
            let ctx = root.ctx().clone();
            if take_focus {
                ctx.send_viewport_cmd(ViewportCommand::Focus);
            }
            let frame = theme::popup(edge).inner_margin(egui::Margin::symmetric(12, 10));
            egui::CentralPanel::default().frame(frame).show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Translate").font(theme::heading_font(16.0)).color(theme::GOLD));
                    let edit = ui.add(theme::field(reply));
                    if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submit = !reply.trim().is_empty();
                    } else {
                        // The box is only ever open to be typed in.
                        edit.request_focus();
                    }
                });
                if let Some((note, color)) = &problem {
                    ui.horizontal(|ui| {
                        if too_long {
                            copy_anyway = ui.add(theme::button("Copy anyway").small()).clicked();
                        }
                        ui.add(egui::Label::new(RichText::new(note).size(11.5).color(*color)).truncate());
                    });
                }
                // A border's width and the frame's margin below the last row. A box shorter
                // than its contents would lose its bottom border.
                height = ui.cursor().top() - ui.spacing().item_spacing.y + 12.0;
                close = ui.input(|i| i.key_pressed(egui::Key::Escape) || i.viewport().close_requested());
            });
        });
        self.reply_box_height = height;
        // A box that did not open this pass still has to take the keyboard when it does.
        self.reply_box_focus = take_focus && !shown;
        if copy_anyway {
            self.copy_anyway(ctx);
        }
        if submit {
            let english = std::mem::take(&mut self.reply).trim().to_string();
            self.send_reply(english);
        }
        if submit || close || copy_anyway {
            self.reply_box = false;
            crate::focus::activate_wow();
        }
    }

    /// What the overlay's pill says about the last reply: its text, color and whether it
    /// is still under way. Nothing while there is nothing to say.
    fn reply_pill(&mut self, ctx: &egui::Context) -> Option<(String, Color32, bool)> {
        if let ReplyState::Copied(since) = self.reply_state {
            match COPIED_SHOWN.checked_sub(since.elapsed()) {
                Some(left) => ctx.request_repaint_after(left),
                None => self.reply_state = ReplyState::Idle,
            }
        }
        match &self.reply_state {
            ReplyState::Idle => None,
            ReplyState::Translating => Some(("Translating".into(), theme::TEXT, true)),
            ReplyState::Copied(_) => Some(("Copied, paste it".into(), theme::GOOD, false)),
            ReplyState::Failed(_) => Some(("Not translated: press Translate".into(), theme::BAD, false)),
            ReplyState::TooLong(_) => Some(("Too long: press Translate".into(), theme::GOLD, false)),
        }
    }

    /// Hides this window and gives the keyboard back to the game. The overlay stays.
    fn hide_window(&mut self, ctx: &egui::Context) {
        let root = ViewportId::ROOT;
        self.window_hidden = true;
        self.window_summoned = false;
        ctx.send_viewport_cmd_to(root, ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
        ctx.send_viewport_cmd_to(root, ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd_to(root, ViewportCommand::Visible(false));
        crate::focus::activate_wow();
    }

    /// Shows, hides and brings back this window: over the game, next to the chat window,
    /// when the game's App button asks for it, and away again on a second click.
    fn window_visibility(&mut self, ctx: &egui::Context) {
        let root = ViewportId::ROOT;
        // A minimized window (Cmd+M, or the Window menu) does not come back on a click
        // on the app's icon as long as the companion is the app in front. Hiding it
        // instead behaves the same as the Hide button, which does. egui does not say
        // "minimized" on macOS while running, but a minimized window has no rectangle.
        let has_rect = ctx.input(|i| i.viewport().inner_rect.is_some());
        self.window_placed |= has_rect;
        if self.window_placed && !has_rect && !self.window_hidden {
            self.hide_window(ctx);
        }
        // A click on the app's icon makes the companion the app in front, but macOS
        // leaves its window hidden, because the overlay counts as a window already
        // showing. So the window is brought back here. Opening the reply box also puts
        // the companion in front, and must not bring it back.
        let active = crate::focus::app_is_active().unwrap_or(false);
        let activated = active && !self.was_active && !self.reply_box;
        self.was_active = active;
        if self.window_hidden {
            // Nothing else may ask for a pass while the app waits to be clicked.
            ctx.request_repaint_after(Duration::from_millis(300));
        }
        let asked = std::mem::take(&mut self.show_window);
        if asked && !self.window_hidden && (self.window_summoned || active) {
            self.hide_window(ctx);
            return;
        }
        if !asked && !(self.window_hidden && activated) {
            return;
        }
        self.window_hidden = false;
        ctx.send_viewport_cmd_to(root, ViewportCommand::Visible(true));
        ctx.send_viewport_cmd_to(root, ViewportCommand::Minimized(false));
        if asked {
            // From the game: stay above it, next to the chat window.
            self.window_summoned = true;
            ctx.send_viewport_cmd_to(root, ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
            if let Some(window) = layout::anchor(&self.chat_windows, &self.draft.anchor_tab) {
                let [x, y, width, height] = window.rect;
                let own = ctx.input(|i| i.viewport().outer_rect).map_or(START_SIZE[1], |r| r.height());
                let top = (y + height - own).max(y - 300.0);
                ctx.send_viewport_cmd_to(root, ViewportCommand::OuterPosition(egui::pos2(x + width + 24.0, top)));
            }
        }
        ctx.send_viewport_cmd_to(root, ViewportCommand::Focus);
    }

    /// Settings take effect as they are changed, and are written to the file once they
    /// have been left alone for a moment, so dragging a slider does not write it at
    /// every step.
    fn keep_settings(&mut self, ctx: &egui::Context) {
        if self.draft == self.saved {
            return;
        }
        // The translators read this copy, so a new backend or key is used at once.
        *self.config.lock().unwrap() = self.draft.clone();
        let since = *self.changed_at.get_or_insert_with(Instant::now);
        match SAVE_AFTER.checked_sub(since.elapsed()) {
            Some(left) => ctx.request_repaint_after(left),
            None => self.save(),
        }
    }

    /// Asks before putting the settings back as they were when the companion started.
    fn undo_question(&mut self, ctx: &egui::Context) {
        if !self.undo_asked {
            return;
        }
        let frame = theme::popup(theme::BORDER).inner_margin(18);
        let asked = egui::Modal::new(egui::Id::new("undo")).frame(frame).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.label(RichText::new("Undo changes").font(theme::heading_font(18.0)).color(theme::GOLD));
            ui.label("This will reset the settings to what they were when the companion was opened. Proceed?");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add(theme::button("Reset settings")).clicked() {
                    self.draft = self.opened.clone();
                    self.custom_endpoint = self.draft.endpoint_preset().is_none();
                    self.custom_claude_model = self.draft.claude_preset().is_none();
                    self.last_anchor = None;
                    self.undo_asked = false;
                }
                if ui.add(theme::button("Cancel")).clicked() {
                    self.undo_asked = false;
                }
            });
        });
        if asked.should_close() {
            self.undo_asked = false;
        }
    }

    fn save(&mut self) {
        // While the overlay sits on a chat tab, overlay_rect keeps the hand-placed spot.
        if let Some(rect) = self.overlay_rect {
            self.draft.overlay_x = rect.min.x;
            self.draft.overlay_y = rect.min.y;
        }
        *self.config.lock().unwrap() = self.draft.clone();
        self.saved = self.draft.clone();
        self.changed_at = None;
        if let Err(e) = config::save(&self.draft) {
            self.notice = format!("Could not save settings: {e}");
        }
    }

    /// The window keeps the size it has, whichever tab is open: resizing it to each
    /// tab's contents showed for a moment at the old size. A tab taller than the window
    /// scrolls.
    fn control_panel(&mut self, root: &mut egui::Ui) {
        let frame = egui::Frame::NONE.fill(theme::BACKGROUND);
        egui::CentralPanel::default().frame(frame).show(root, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| self.control_contents(ui));
        });
    }

    fn control_contents(&mut self, ui: &mut egui::Ui) {
        // The title bar, the tabs and the page touch, with no gap showing between them.
        let spacing = std::mem::replace(&mut ui.spacing_mut().item_spacing.y, 0.0);
        self.title_bar(ui);
        self.tab_bar(ui);
        egui::Frame::NONE.inner_margin(egui::Margin::symmetric(18, 14)).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = spacing;
            // Text takes the window's width and wraps; nothing is cut off at the edge.
            ui.set_width(ui.available_width());
            if !self.notice.is_empty() {
                ui.label(RichText::new(&self.notice).color(theme::MUTED));
                ui.add_space(4.0);
            }
            match self.tab {
                Tab::Translate => self.translate_tab(ui),
                Tab::Backend => self.backend_tab(ui),
                Tab::Overlay => self.overlay_tab(ui),
                Tab::Status => self.status_tab(ui),
                Tab::History => self.history_tab(ui),
            }
        });
    }

    /// The name, whether translation is working and with what, and the window's own buttons.
    fn title_bar(&mut self, ui: &mut egui::Ui) {
        // Dragging the bar moves the window, as a title bar does. Asked for before the
        // bar's contents are added, so its buttons still get their clicks.
        let whole = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), TITLE_BAR_HEIGHT));
        if ui.interact(whole, ui.id().with("move window"), egui::Sense::drag()).drag_started() {
            ui.ctx().send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::StartDrag);
        }
        let bar = egui::Frame::NONE.fill(theme::BAR).inner_margin(egui::Margin::symmetric(14, 8));
        bar.show(ui, |ui| {
            ui.horizontal(|ui| {
                theme::title(ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(theme::button("X")).on_hover_text("Quit the companion. The overlay goes too.").clicked() {
                        self.save();
                        self.write_backlog();
                        ui.ctx().send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
                    }
                    let hint = "Hide this window; the overlay stays. The game's App button or the app's icon brings it back.";
                    if ui.add(theme::button("Hide")).on_hover_text(hint).clicked() {
                        self.hide_window(ui.ctx());
                    }
                    if ui.add(theme::button("Restart")).on_hover_text("Quit and start the companion again.").clicked() {
                        self.save();
                        self.write_backlog();
                        match config::start_again() {
                            Ok(()) => ui.ctx().send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close),
                            Err(e) => self.notice = format!("Could not restart: {e}"),
                        }
                    }
                    let changed = self.draft != self.opened;
                    let undo = ui.add_enabled(changed, theme::button("Undo"));
                    if undo.on_hover_text("Put the settings back as they were when the companion was opened.").clicked() {
                        self.undo_asked = true;
                    }
                    // What is left of the row, so a long status is shortened, not pushed out.
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let working = !self.status.starts_with("Translation failed");
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 4.0, if working { theme::GOOD } else { theme::BAD });
                        // Chat lines leave this computer for the service named here, unless it runs locally.
                        let status = format!("{} · {}", self.status, self.draft.backend.short_label());
                        ui.add(egui::Label::new(status).truncate());
                    });
                });
            });
        });
    }

    /// The tabs stand on a line; the chosen one is open at the bottom, joined to its page.
    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let bar = egui::Frame::NONE.fill(theme::BAR).inner_margin(egui::Margin { left: 14, right: 14, top: 4, bottom: 0 });
        let mut chosen = egui::Rect::NOTHING;
        let row = bar.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (tab, name) in Tab::ALL {
                    let selected = self.tab == tab;
                    let color = if selected { Color32::WHITE } else { theme::GOLD };
                    let text = ui.painter().layout_no_wrap(name.to_string(), theme::heading_font(15.0), color);
                    let (rect, response) = ui.allocate_exact_size(text.size() + egui::vec2(28.0, 14.0), egui::Sense::click());
                    // The other tabs are a little lower and darker.
                    let shape = if selected { rect } else { rect.with_min_y(rect.top() + 3.0) };
                    let (fill, edge) = match (selected, response.hovered()) {
                        (true, _) => (theme::BACKGROUND, theme::BORDER),
                        (false, true) => (theme::BACKGROUND, theme::BORDER_FIELD),
                        (false, false) => (theme::FIELD, theme::BORDER_DIM),
                    };
                    let corners = egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 };
                    ui.painter().rect(shape, corners, fill, egui::Stroke::new(1.0, edge), egui::StrokeKind::Inside);
                    ui.painter().galley(shape.center() - text.size() / 2.0, text, color);
                    if selected {
                        chosen = rect;
                    }
                    if response.clicked() {
                        self.tab = tab;
                    }
                }
                ui.add_space(ui.available_width());
            });
        });
        let bottom = row.response.rect.bottom();
        let painter = ui.painter();
        painter.hline(row.response.rect.x_range(), bottom - 0.5, egui::Stroke::new(1.0, theme::BORDER));
        // Rub the line out under the chosen tab, between its side edges.
        let gap = (chosen.left() + 1.0)..=(chosen.right() - 1.0);
        painter.hline(gap, bottom - 0.5, egui::Stroke::new(1.0, theme::BACKGROUND));
    }

    fn translate_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(theme::label("Translate into Chinese"));
        let edit = ui.add(
            theme::field(&mut self.reply).hint_text("Type in English, press Enter, then paste into WoW chat"),
        );
        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.reply.trim().is_empty() {
            let english = std::mem::take(&mut self.reply).trim().to_string();
            self.send_reply(english);
            self.notice = "Translating your reply".into();
            edit.request_focus();
        }
        if let Some((problem, color)) = self.reply_problem() {
            ui.label(RichText::new(problem).color(color));
        }
        if matches!(self.reply_state, ReplyState::TooLong(_)) && ui.add(theme::button("Copy anyway")).clicked() {
            self.copy_anyway(ui.ctx());
        }
    }

    fn backend_tab(&mut self, ui: &mut egui::Ui) {
        let wide = ui.available_width();
        ui.label(theme::label("Translate with"));
        egui::ComboBox::from_id_salt("backend").width(wide).selected_text(self.draft.backend.label()).show_ui(ui, |ui| {
            for backend in Backend::ALL {
                ui.selectable_value(&mut self.draft.backend, backend, backend.label());
            }
        });
        match self.draft.backend {
            Backend::GoogleFree => {
                ui.label(RichText::new("No slang hints. Rate-limited and service might be disrupted.").color(theme::MUTED));
            }
            Backend::Claude => {
                ui.label(theme::label("Claude API key"));
                ui.add(theme::field(&mut self.draft.claude_api_key).password(true));
                if let Some(problem) = key_problem(&self.draft.claude_api_key) {
                    ui.label(RichText::new(problem).color(theme::BAD));
                }
                ui.label(theme::label("Claude model"));
                let chosen = self.draft.claude_preset().filter(|_| !self.custom_claude_model);
                let labels: Vec<&str> = CLAUDE_MODELS.iter().map(|(label, _)| *label).collect();
                if let Some(picked) = preset_combo(ui, "claude_model", &labels, chosen) {
                    self.custom_claude_model = picked.is_none();
                    if let Some(index) = picked {
                        self.draft.claude_model = CLAUDE_MODELS[index].1.to_string();
                    }
                }
                if chosen.is_none() {
                    ui.label(theme::label("Model ID"));
                    ui.add(theme::field(&mut self.draft.claude_model));
                }
            }
            Backend::OpenAiCompatible => {
                ui.label(theme::label("Service and model"));
                let chosen = self.draft.endpoint_preset().filter(|_| !self.custom_endpoint);
                let labels: Vec<&str> = ENDPOINT_PRESETS.iter().map(|preset| preset.label).collect();
                if let Some(picked) = preset_combo(ui, "endpoint", &labels, chosen) {
                    self.custom_endpoint = picked.is_none();
                    if let Some(index) = picked {
                        self.draft.api_base = ENDPOINT_PRESETS[index].api_base.to_string();
                        self.draft.model = ENDPOINT_PRESETS[index].model.to_string();
                    }
                }
                if chosen.is_none() {
                    ui.label(theme::label("API base URL"));
                    ui.add(theme::field(&mut self.draft.api_base));
                    ui.label(theme::label("Model"));
                    ui.add(theme::field(&mut self.draft.model));
                }
                ui.label(theme::label("API key"));
                ui.add(theme::field(&mut self.draft.api_key).password(true).hint_text("empty for Ollama"));
            }
        }
        ui.label(theme::label("Server"));
        egui::ComboBox::from_id_salt("server").width(wide).selected_text(self.draft.server.server_label()).show_ui(ui, |ui| {
            for region in Region::ALL {
                ui.selectable_value(&mut self.draft.server, region, region.server_label());
            }
        });
        ui.label(theme::label("Reply in"));
        egui::ComboBox::from_id_salt("reply_variant")
            .width(wide)
            .selected_text(self.draft.reply_variant.reply_label())
            .show_ui(ui, |ui| {
                for region in Region::ALL {
                    ui.selectable_value(&mut self.draft.reply_variant, region, region.reply_label());
                }
            });
        // For the [?] link in the game, which explains a Chinese line word by word.
        ui.label(theme::label("Explain words with"));
        egui::ComboBox::from_id_salt("reading").width(wide).selected_text(self.draft.reading.label()).show_ui(ui, |ui| {
            for reading in Reading::ALL {
                ui.selectable_value(&mut self.draft.reading, reading, reading.label());
            }
        });

        ui.add_space(6.0);
        theme::boxed().show(ui, |ui| {
            ui.set_width(ui.available_width());
            if ui.add(theme::button("Test translation")).clicked() {
                *self.config.lock().unwrap() = self.draft.clone();
                let _ = self.interactive.send(Job::Test);
                self.test_result = format!("Testing {}", self.draft.backend.label());
            }
            if !self.test_result.is_empty() {
                ui.label(&self.test_result);
            }
        });
    }

    fn overlay_tab(&mut self, ui: &mut egui::Ui) {
        if ui.add(theme::button("Clear overlay")).clicked() {
            self.lines.clear();
            self.backlog_changed = true;
        }
        ui.checkbox(&mut self.draft.show_original, "Show Chinese too");
        let can_name = self.draft.backend.takes_instructions();
        let label = if can_name { "Translate player names" } else { "Translate player names (unavailable with Google Translate)" };
        ui.add_enabled(can_name, egui::Checkbox::new(&mut self.draft.translate_names, label))
            .on_hover_text("After a Chinese name, show what it means, or how it sounds if it means nothing.");
        ui.checkbox(&mut self.draft.hide_when_unfocused, HIDE_LABEL);
        ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(120.0);
        slider(ui, "Lines kept (scrollback)", &mut self.draft.max_lines, 1..=500);

        self.source_picker(ui);
        if cover::AVAILABLE {
            ui.checkbox(&mut self.draft.cover_strip, "Cover the pixel strip")
                .on_hover_text("Puts a dark patch over the strip in the top-left corner. You see the patch; the companion still reads the strip, since screen captures leave the patch out.");
        }

        // On a chat tab the overlay takes the tab's place, size, text size and
        // background. Placed by hand, it has its own, and can be unlocked to drag it.
        let mut anchoring = !self.draft.anchor_tab.trim().is_empty();
        if ui.checkbox(&mut anchoring, "Anchor to chat tab").changed() {
            if anchoring {
                // Back on the tab used last, or failing that the game's first.
                let first = self.chat_windows.first().map(|window| window.name.clone());
                self.draft.anchor_tab = std::mem::take(&mut self.last_anchor).or(first).unwrap_or_default();
                self.locked = true;
                if self.draft.anchor_tab.is_empty() {
                    self.notice = "The addon has not reported the game's chat tabs yet, so there is none to anchor to.".into();
                }
            } else {
                self.last_anchor = Some(std::mem::take(&mut self.draft.anchor_tab));
            }
        }
        if !self.draft.anchor_tab.trim().is_empty() {
            ui.label(theme::label("Chat tab"));
            let chosen = self.draft.anchor_tab.clone();
            let known = self.chat_windows.iter().any(|window| window.name == chosen);
            egui::ComboBox::from_id_salt("anchor_tab").width(ui.available_width()).selected_text(&chosen).show_ui(ui, |ui| {
                for window in &self.chat_windows {
                    ui.selectable_value(&mut self.draft.anchor_tab, window.name.clone(), &window.name);
                }
                if !known {
                    // A tab saved earlier that the game does not have now stays listed.
                    ui.selectable_value(&mut self.draft.anchor_tab, chosen.clone(), format!("{chosen} (not in the game now)"));
                }
            });
            // Only said when the overlay is not simply showing on the chosen tab.
            if let Some(problem) = self.anchor_status() {
                ui.label(RichText::new(problem).color(theme::MUTED));
            }
            slider(ui, "Background opacity", &mut self.draft.anchored_opacity, 0.0..=1.0);
        } else {
            let label = if self.locked { "Unlock overlay to move it" } else { "Lock overlay" };
            if ui.add(theme::button(label)).clicked() {
                self.locked = !self.locked;
                if self.locked {
                    self.save();
                }
            }
            slider(ui, "Overlay width", &mut self.draft.overlay_width, 200.0..=1600.0);
            slider(ui, "Overlay height", &mut self.draft.overlay_height, 80.0..=1000.0);
            slider(ui, "Background opacity", &mut self.draft.overlay_opacity, 0.0..=1.0);
            slider(ui, "Text size", &mut self.draft.font_size, 10.0..=32.0);
        }
    }

    /// Which chat window the overlay gets the lines of. The choice lives in the game: the
    /// companion cannot reach the addon, so picking a window copies the /tr /window
    /// command for the player to paste into chat, and the addon's report shows when it
    /// has taken effect.
    fn source_picker(&mut self, ui: &mut egui::Ui) {
        ui.label(theme::label("Chat to translate"));
        let shown = |name: &str| if name.is_empty() { "All chat".to_string() } else { format!("The \"{name}\" window") };
        let current = self.source_window.clone();
        let mut picked: Option<String> = None;
        let selected = current.as_deref().map_or("Not reported yet".to_string(), shown);
        egui::ComboBox::from_id_salt("source_window").width(ui.available_width()).selected_text(selected).show_ui(ui, |ui| {
            let choices = std::iter::once(String::new()).chain(self.chat_windows.iter().map(|window| window.name.clone()));
            for name in choices {
                if ui.selectable_label(current.as_deref() == Some(name.as_str()), shown(&name)).clicked() {
                    picked = Some(name);
                }
            }
        });
        if let Some(name) = picked.filter(|name| current.as_ref() != Some(name)) {
            let command = source_command(&name);
            ui.ctx().copy_text(command.clone());
            self.notice = format!("Copied \"{command}\". Paste it into WoW chat and press Enter to switch.");
        }
        let note = match &self.source_window {
            None if self.chat_windows.is_empty() => "Waiting for the addon to report the chat windows.",
            None => "The addon has not said which window it copies: update it to the companion's version and /reload.",
            Some(_) => "Picking another copies a /tr command to paste into WoW chat; the game makes the switch.",
        };
        ui.label(RichText::new(note).color(theme::MUTED));
    }

    /// The strip cover (see `cover`): a dark window over the strip that the player sees
    /// and screen captures leave out. It is opened parked off screen, left out of captures,
    /// and only then moved onto the strip; if leaving it out fails it stays parked. It goes
    /// with the strip, and away while WoW is out of sight, like the overlay.
    fn strip_cover(&mut self, ctx: &egui::Context) {
        if !cover::AVAILABLE || !self.draft.cover_strip {
            return;
        }
        // Opened once the strip has been found, and kept: opening a window again can take
        // focus from the game.
        if self.strip_place.is_none() && self.cover_sent.is_none() {
            return;
        }
        // Checked each pass, before the cover is placed: a window that captures would see
        // must never sit on the strip. Before the window first opens this is false, so it
        // opens parked.
        let hidden_from_capture = cover::hide_from_capture();
        if hidden_from_capture != self.cover_hidden_from_capture {
            crate::diag::log(if hidden_from_capture {
                "strip cover: left out of screen captures"
            } else {
                "strip cover: not left out of screen captures, so kept off the strip"
            });
            self.cover_hidden_from_capture = hidden_from_capture;
        }
        let out_of_sight = self.draft.hide_when_unfocused && matches!(self.wow_view, WowView::Hidden);
        let place = self.strip_place.filter(|_| self.cover_hidden_from_capture && !out_of_sight);
        let pos = place.map_or(egui::pos2(OFF_SCREEN, OFF_SCREEN), |rect| egui::pos2(rect[0], rect[1]));
        let size = place.map_or([128.0, 18.0], |rect| [rect[2], rect[3]]);
        let builder = ViewportBuilder::default()
            .with_title(cover::TITLE)
            .with_decorations(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_active(false)
            .with_mouse_passthrough(true)
            .with_resizable(false)
            .with_position(egui::pos2(OFF_SCREEN, OFF_SCREEN))
            .with_inner_size(size);
        if self.cover_sent != Some(pos) {
            ctx.send_viewport_cmd_to(cover_id(), ViewportCommand::OuterPosition(pos));
            ctx.send_viewport_cmd_to(cover_id(), ViewportCommand::InnerSize(egui::vec2(size[0], size[1])));
            self.cover_sent = Some(pos);
        }
        let shown = show_window(ctx, cover_id(), builder, |root, _class| {
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(COVER_COLOR)).show(root, |_| {});
        });
        if !shown || !self.cover_hidden_from_capture {
            // The window may not be up yet in its first pass: tried again shortly.
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    /// The lines the overlay holds, to scroll through and select text from.
    fn history_tab(&mut self, ui: &mut egui::Ui) {
        theme::boxed().show(ui, |ui| {
            let room = (ui.available_height() - 60.0).max(160.0);
            let scroll = egui::ScrollArea::vertical().id_salt("history").max_height(room).stick_to_bottom(true);
            scroll.auto_shrink([false, true]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                let font = egui::FontId::new(14.0, egui::FontFamily::Name(crate::fonts::OVERLAY_FAMILY.into()));
                let width = ui.available_width();
                let names = self.names_shown().then_some(&self.names);
                if self.lines.is_empty() {
                    ui.label(RichText::new("No chat lines yet.").color(theme::MUTED));
                }
                for line in &self.lines {
                    ui.add(egui::Label::new(line.job(&font, width, names)).selectable(true));
                }
            });
        });
        ui.label(RichText::new("Holds as many lines as \"Lines kept\" on the Overlay tab.").color(theme::MUTED));
    }

    /// How things stand, for a report of a problem: what the Status tab shows and the
    /// settings that bear on it. No keys and no chat.
    fn state_for_report(&self) -> String {
        let draft = &self.draft;
        let model = match draft.backend {
            Backend::GoogleFree => String::new(),
            Backend::Claude => format!(", model {}", draft.claude_model),
            Backend::OpenAiCompatible => format!(", model {} at {}", draft.model, draft.api_base),
        };
        format!(
            "System: {} {}\nBackend: {}{model}\nTranslation: {}\nPixel strip: {}\nChat windows: {:?}\nOverlay: on tab {:?}, hidden {}, game in view {:?}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            draft.backend.short_label(),
            self.status,
            self.strip_status,
            self.chat_windows,
            draft.anchor_tab,
            self.overlay_hidden,
            self.wow_view,
        )
    }

    fn status_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(theme::label("Translation"));
        ui.label(format!("{} · translating with {}", self.status, self.draft.backend.short_label()));
        ui.label(theme::label("Pixel strip"));
        ui.label(&self.strip_status);
        ui.label(theme::label("Chat windows"));
        if self.chat_windows.is_empty() {
            ui.label("Waiting for the addon to report the chat windows.");
        } else {
            let names: Vec<&str> = self.chat_windows.iter().map(|window| window.name.as_str()).collect();
            ui.label(names.join(", "));
        }
        ui.label(theme::label("Settings file"));
        ui.label(config::path().display().to_string());
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.add(theme::button("Copy report")).clicked() {
                ui.ctx().copy_text(crate::diag::report(&self.state_for_report()));
                self.report_copied = true;
            }
            let note = if self.report_copied {
                "Copied. Paste it where you report the problem."
            } else {
                "The lines above and the end of the log. No keys, no chat."
            };
            ui.label(RichText::new(note).color(theme::MUTED));
        });
    }

    /// Why the overlay is not showing on the chosen tab, if it is not.
    fn anchor_status(&self) -> Option<String> {
        match layout::anchor(&self.chat_windows, &self.draft.anchor_tab) {
            Some(window) if window.shown => None,
            Some(window) => Some(format!("Hidden until the \"{}\" tab is selected in the game.", window.name)),
            None if self.chat_windows.is_empty() => Some("Waiting for the addon to report the chat tabs.".into()),
            None => Some("The game has no such tab now, so the overlay is placed by hand.".into()),
        }
    }

    fn overlay(&mut self, ctx: &egui::Context) {
        // On a chat tab the overlay takes that window's place, size and font size.
        let anchor = layout::anchor(&self.chat_windows, &self.draft.anchor_tab).cloned();
        let size = match &anchor {
            Some(window) => [window.rect[2].max(40.0), window.rect[3].max(20.0)],
            None => [self.draft.overlay_width, self.draft.overlay_height],
        };
        let locked = self.locked;
        // A hidden overlay is not closed or hidden: showing a window again can take
        // focus from WoW. On macOS it stays where it is and draws nothing. It must stay
        // on screen there: the app only runs while one of its windows can be seen, so
        // with the overlay parked and this window behind the game, nothing would run
        // and the overlay would never come back. Elsewhere it is parked off screen,
        // because Windows draws a border around an empty transparent window.
        let area = match &anchor {
            Some(window) => Some(window.rect),
            None => self.overlay_rect.map(|r| [r.min.x, r.min.y, r.width(), r.height()]),
        };
        let covered = locked && self.draft.hide_when_unfocused && !self.wow_view.shows_overlay(area);
        // Until the addon has reported the chat windows, which takes a few seconds after
        // the companion starts, an overlay meant for a chat tab has no place yet; it is
        // not shown where it was last put by hand meanwhile.
        let waiting = !self.draft.anchor_tab.trim().is_empty() && self.chat_windows.is_empty();
        let hidden = covered || waiting || anchor.as_ref().is_some_and(|window| !window.shown);
        let builder = ViewportBuilder::default()
            .with_title("Translate overlay")
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_mouse_passthrough(self.locked || hidden)
            .with_position(self.overlay_start)
            .with_inner_size(size);

        let pos = match &anchor {
            _ if hidden && PARK_HIDDEN_OVERLAY => egui::pos2(OFF_SCREEN, OFF_SCREEN),
            Some(window) => egui::pos2(window.rect[0], window.rect[1]),
            None => self.overlay_rect.map_or(self.overlay_start, |r| r.min),
        };
        // Placed by hand, the window is only moved to park and unpark it, so dragging it
        // is not fought. On a chat tab it follows the tab.
        let changed_mode = hidden != self.overlay_hidden || anchor.is_some() != self.overlay_anchored;
        if changed_mode || (anchor.is_some() && self.overlay_sent != Some(pos)) {
            ctx.send_viewport_cmd_to(overlay_id(), ViewportCommand::OuterPosition(pos));
            self.overlay_sent = Some(pos);
            self.overlay_hidden = hidden;
            self.overlay_anchored = anchor.is_some();
        }
        let mut draft = self.draft.clone();
        if let Some(window) = &anchor {
            draft.font_size = window.font_size.clamp(6.0, 48.0);
            // The chat window behind the overlay draws its own background; this one is
            // set apart, and none by default.
            draft.overlay_opacity = self.draft.anchored_opacity;
        }
        let anchored = anchor.is_some();
        let mut rect = self.overlay_rect;
        let wheel_steps = std::mem::take(&mut self.wheel_steps);
        let pill = self.reply_pill(ctx);
        let lines = &self.lines;
        let names = self.names_shown().then_some(&self.names);

        show_window(ctx, overlay_id(), builder, |root, _class| {
            let ctx = root.ctx().clone();
            if hidden {
                return;
            }
            // Skip the parked position, which can linger for a frame after showing.
            if !anchored {
                rect = ctx.input(|i| i.viewport().outer_rect).filter(|r| r.min.x > OFF_SCREEN / 2.0).or(rect);
            }
            let alpha = (draft.overlay_opacity * 255.0) as u8;
            let fill = if locked {
                Color32::from_black_alpha(alpha)
            } else {
                Color32::from_rgba_unmultiplied(20, 60, 110, alpha.max(140))
            };
            let frame = egui::Frame::NONE.fill(fill).inner_margin(8.0);
            egui::CentralPanel::default().frame(frame).show(root, |ui| {
                if !locked && anchored {
                    ui.label(RichText::new("Following the chat tab. Lock it from the companion window.").color(Color32::WHITE));
                } else if !locked {
                    let drag = ui.interact(ui.max_rect(), ui.id().with("drag"), egui::Sense::drag());
                    if drag.drag_started() {
                        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                    }
                    ui.label(RichText::new("Drag to move. Lock it from the companion window.").color(Color32::WHITE));
                }
                let room = ui.available_height();
                egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink(false).show(ui, |ui| {
                    // Chat lines sit tight together and fill the window from the bottom up.
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let font = egui::FontId::new(draft.font_size, egui::FontFamily::Name(crate::fonts::OVERLAY_FAMILY.into()));
                    let small = egui::FontId::new(draft.font_size * 0.85, font.family.clone());
                    let width = ui.available_width() - SHADOW;
                    if wheel_steps != 0.0 {
                        // Wheel up moves the lines down, bringing earlier ones into view.
                        let line_height = ui.fonts_mut(|fonts| fonts.row_height(&font));
                        ui.scroll_with_delta(egui::vec2(0.0, wheel_steps * WHEEL_LINES * line_height));
                    }
                    let mut galleys = Vec::new();
                    for line in lines {
                        galleys.push(ui.painter().layout_job(line.job(&font, width, names)));
                        let original = markup::plain(&line.line.body);
                        // Not under a line that had no body of its own, such as a name explained.
                        if draft.show_original && matches!(line.translation, Translation::Done(_)) && !original.trim().is_empty() {
                            let format = TextFormat::simple(small.clone(), Color32::from_gray(170));
                            let mut job = LayoutJob::single_section(original, format);
                            job.wrap.max_width = width;
                            galleys.push(ui.painter().layout_job(job));
                        }
                    }
                    let height: f32 = galleys.iter().map(|galley| galley.size().y).sum();
                    ui.add_space((room - height - SHADOW).max(0.0));
                    for galley in galleys {
                        let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
                        // The game draws chat with a dark shadow one step down and right.
                        let shadow = rect.min + egui::vec2(SHADOW, SHADOW);
                        ui.painter().galley_with_override_text_color(shadow, galley.clone(), Color32::BLACK);
                        ui.painter().galley(rect.min, galley, Color32::WHITE);
                    }
                });
                if let Some((text, color, busy)) = &pill {
                    // In the top right corner, over the oldest lines.
                    let galley = ui.painter().layout_no_wrap(text.clone(), egui::FontId::proportional(12.0), *color);
                    let (pad, spinner) = (egui::vec2(8.0, 5.0), if *busy { 17.0 } else { 0.0 });
                    let size = galley.size() + 2.0 * pad + egui::vec2(spinner, 0.0);
                    let corner = ui.max_rect().right_top();
                    let pill = egui::Rect::from_min_size(egui::pos2(corner.x - size.x, corner.y), size);
                    let edge = egui::Stroke::new(1.0, *color);
                    ui.painter().rect(pill, 4, theme::BACKGROUND, edge, egui::StrokeKind::Inside);
                    if *busy {
                        let at = egui::Rect::from_center_size(egui::pos2(pill.left() + pad.x + 6.0, pill.center().y), egui::vec2(12.0, 12.0));
                        // A turning arc, redrawn a dozen times a second: egui's own spinner
                        // asks for a new pass of every window as fast as they can be drawn.
                        let turn = ui.input(|i| i.time) as f32 * std::f32::consts::TAU;
                        let points = (0..=12).map(|step| at.center() + 5.0 * egui::Vec2::angled(turn + step as f32 * 0.4));
                        ui.painter().add(egui::Shape::line(points.collect(), egui::Stroke::new(2.0, theme::GOLD)));
                        ui.ctx().request_repaint_after(Duration::from_millis(80));
                    }
                    ui.painter().galley(pill.min + pad + egui::vec2(spinner, 0.0), galley, *color);
                }
            });
        });
        self.overlay_rect = rect;
    }
}

impl eframe::App for TranslateApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.passes.begin();
        self.drain_events();
        self.ask_for_names();
        self.control_panel(ui);
        self.window_visibility(&ctx);
        // A copy started only to be pictured must not put a second overlay over the game.
        if std::env::var_os("TRANSLATE_SHOTS").is_none() {
            self.overlay(&ctx);
            self.strip_cover(&ctx);
        }
        self.reply_box(&ctx);
        self.undo_question(&ctx);
        self.keep_settings(&ctx);
        self.keep_backlog(&ctx);
        // After both windows laid out their text, so this frame's new glyphs are included.
        self.refresh_font_texture(&ctx);
        self.shots(&ctx);
        self.passes.end(&ctx, || {
            format!(
                "tab {}, reply box {}, translating {}, window hidden {}, overlay hidden {}, {} lines",
                Tab::ALL.iter().position(|(tab, _)| *tab == self.tab).unwrap_or(0),
                self.reply_box,
                matches!(self.reply_state, ReplyState::Translating),
                self.window_hidden,
                self.overlay_hidden,
                self.lines.len()
            )
        });
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Core.lua's /tr /window command takes these: a window's name, or "all".
    #[test]
    fn source_commands_match_the_addon() {
        assert_eq!(source_command("General"), "/tr /window General");
        assert_eq!(source_command("Guild Chat"), "/tr /window Guild Chat");
        assert_eq!(source_command(""), "/tr /window all");
    }

    #[test]
    fn a_late_line_goes_before_the_lines_that_followed_it() {
        let lines = [Some(10), None, Some(12), Some(14), None];
        assert_eq!(place(lines.into_iter(), Some(15)), 5, "the newest goes last");
        assert_eq!(place(lines.into_iter(), Some(13)), 3);
        assert_eq!(place(lines.into_iter(), Some(11)), 2, "after a line the player asked for at that time");
        assert_eq!(place(lines.into_iter(), Some(5)), 0);
        assert_eq!(place(lines.into_iter(), None), 5, "a line the player asked for goes last");
    }

    /// The overlay scrolls by calling `scroll_with_delta` inside its scroll area; this
    /// checks that doing so moves a scroll area that sticks to the bottom, and which way.
    #[test]
    fn wheel_steps_scroll_back_through_earlier_lines() {
        let ctx = egui::Context::default();
        let offset = |steps: f32| {
            let mut offset = 0.0;
            let mut output = ctx.run_ui(Default::default(), |ui| {
                ui.set_max_height(100.0);
                let area = egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink(false).animated(false);
                let shown = area.show(ui, |ui| {
                    if steps != 0.0 {
                        ui.scroll_with_delta(egui::vec2(0.0, steps * 30.0));
                    }
                    ui.allocate_exact_size(egui::vec2(50.0, 1000.0), egui::Sense::hover());
                });
                offset = shown.state.offset.y;
            });
            output.textures_delta.clear();
            offset
        };
        offset(0.0);
        let bottom = offset(0.0);
        assert!(bottom > 800.0, "starts at the bottom: {bottom}");
        let up = offset(2.0);
        assert!((bottom - up - 60.0).abs() < 1.0, "two steps up show earlier lines: {bottom} -> {up}");
        assert!((offset(0.0) - up).abs() < 1.0, "and it stays there");
        assert!((offset(-5.0) - bottom).abs() < 1.0, "scrolling down stops at the bottom");
    }
}
