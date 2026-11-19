use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Which Chinese: of the server played on, whose slang to expect in chat, and of the
/// replies written for the player.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    /// Traditional characters with Taiwanese wording. A Taiwanese server has mainland
    /// players too.
    #[default]
    #[serde(alias = "zh-Hant-TW")]
    Taiwan,
    /// Simplified characters with mainland wording.
    #[serde(alias = "zh-Hans")]
    Mainland,
}

impl Region {
    pub const ALL: [Region; 2] = [Region::Taiwan, Region::Mainland];

    pub fn server_label(self) -> &'static str {
        match self {
            Region::Taiwan => "Taiwan",
            Region::Mainland => "Mainland China",
        }
    }

    pub fn reply_label(self) -> &'static str {
        match self {
            Region::Taiwan => "Traditional Chinese (Taiwan)",
            Region::Mainland => "Simplified Chinese",
        }
    }
}

/// How a Chinese word's pronunciation is written when a line is explained.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reading {
    /// Latin letters with tone marks, as taught on the mainland and abroad.
    #[default]
    Pinyin,
    /// Zhuyin symbols, as taught in Taiwan.
    Bopomofo,
}

impl Reading {
    pub const ALL: [Reading; 2] = [Reading::Pinyin, Reading::Bopomofo];

    pub fn label(self) -> &'static str {
        match self {
            Reading::Pinyin => "Pinyin (fùběn)",
            Reading::Bopomofo => "Bopomofo / Zhuyin (ㄈㄨˋ ㄅㄣˇ)",
        }
    }
}

/// A ready-made OpenAI-compatible endpoint offered in the settings.
pub struct EndpointPreset {
    pub label: &'static str,
    pub api_base: &'static str,
    pub model: &'static str,
}

const OPENAI_BASE: &str = "https://api.openai.com/v1";
const OLLAMA_BASE: &str = "http://localhost:11434/v1";
const OLLAMA_MODEL: &str = "hf.co/tencent/HY-MT1.5-7B-GGUF:Q4_K_M";

pub const ENDPOINT_PRESETS: [EndpointPreset; 3] = [
    EndpointPreset { label: "OpenAI: GPT-4o mini (fast, cheap)", api_base: OPENAI_BASE, model: "gpt-4o-mini" },
    EndpointPreset { label: "OpenAI: GPT-6 Luna", api_base: OPENAI_BASE, model: "gpt-6-luna" },
    EndpointPreset { label: "Ollama: HY-MT 1.5 (on this computer)", api_base: OLLAMA_BASE, model: OLLAMA_MODEL },
];

/// Claude models offered in the settings, as label and model ID. Haiku comes first: a chat
/// line is short and wants a fast answer.
pub const CLAUDE_MODELS: [(&str, &str); 3] = [
    ("Haiku 5.5 (fastest, cheapest; recommended)", "claude-haiku-5-5"),
    ("Sonnet 5.5 (better with slang, slower)", "claude-sonnet-5-5"),
    ("Opus 5.5 (best, slowest, most costly)", "claude-opus-5-5"),
];

/// Label shown for an endpoint or model typed in by hand.
pub const CUSTOM_LABEL: &str = "Custom";

impl Config {
    /// Which preset the OpenAI-compatible settings match, if any.
    pub fn endpoint_preset(&self) -> Option<usize> {
        let base = self.api_base.trim().trim_end_matches('/');
        ENDPOINT_PRESETS.iter().position(|preset| preset.api_base == base && preset.model == self.model.trim())
    }

    /// Which offered Claude model is set, if any.
    pub fn claude_preset(&self) -> Option<usize> {
        CLAUDE_MODELS.iter().position(|(_, id)| *id == self.claude_model.trim())
    }
}

/// The service that translates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    /// Google Translate's free web endpoint: no key, but unofficial and rate-limited.
    #[default]
    GoogleFree,
    /// Claude, with a Claude Console API key.
    Claude,
    /// Any OpenAI-compatible endpoint: Ollama, LM Studio, OpenAI and others.
    #[serde(rename = "openai_compatible")]
    OpenAiCompatible,
}

impl Backend {
    pub const ALL: [Backend; 3] = [Backend::GoogleFree, Backend::Claude, Backend::OpenAiCompatible];

    pub fn label(self) -> &'static str {
        match self {
            Backend::GoogleFree => "Google Translate (free, no key)",
            Backend::Claude => "Claude",
            Backend::OpenAiCompatible => "OpenAI-compatible (Ollama, LM Studio, OpenAI)",
        }
    }

    /// The name without the examples, for the status line.
    pub fn short_label(self) -> &'static str {
        match self {
            Backend::GoogleFree => "Google Translate",
            Backend::Claude => "Claude",
            Backend::OpenAiCompatible => "OpenAI compatible",
        }
    }

    /// Whether the backend can follow instructions: explain a line or a name, and put a
    /// name into English as a name. Google Translate only translates text.
    pub fn takes_instructions(self) -> bool {
        self != Backend::GoogleFree
    }

    /// A config file written before backends existed could only use this one.
    fn before_backends() -> Self {
        Backend::OpenAiCompatible
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// New installs start on Google's free service, which needs no setup. A file without
    /// this field comes from an older version and keeps its OpenAI-compatible endpoint.
    #[serde(default = "Backend::before_backends")]
    pub backend: Backend,
    /// Any OpenAI-compatible base URL, ending before /chat/completions.
    pub api_base: String,
    /// Left empty for local servers such as Ollama.
    pub api_key: String,
    pub model: String,
    /// A Claude Console API key (sk-ant-api...), never a subscription login token.
    pub claude_api_key: String,
    pub claude_model: String,
    /// Where the services are reached. Only tests change these, to use a local stand-in.
    pub google_free_base: String,
    pub claude_base: String,
    /// Whose slang to expect in chat.
    pub server: Region,
    /// The Chinese that replies are written in.
    pub reply_variant: Region,
    pub reading: Reading,
    /// Show what a Chinese player name means, or how it sounds, after it.
    pub translate_names: bool,
    pub overlay_x: f32,
    pub overlay_y: f32,
    pub overlay_width: f32,
    pub overlay_height: f32,
    pub overlay_opacity: f32,
    pub font_size: f32,
    pub max_lines: usize,
    pub show_original: bool,
    /// Name of the chat tab the overlay sits on and follows; empty to place it by hand.
    pub anchor_tab: String,
    /// Background opacity of the overlay on a chat tab, apart from `overlay_opacity`: the
    /// game's chat window behind it draws a background of its own, so none is the default,
    /// but a darker one helps over a bright scene.
    pub anchored_opacity: f32,
    /// Text size of the overlay on a chat tab; 0 takes the tab's own.
    pub anchored_font_size: f32,
    /// Width and height of the overlay on a chat tab; 0 takes the tab's own. The overlay
    /// keeps the tab's bottom-left corner, where chat lines start.
    pub anchored_width: f32,
    pub anchored_height: f32,
    /// Cover the pixel strip with a dark window that screen captures leave out (Windows).
    pub cover_strip: bool,
    /// Hide the locked overlay while another window is in front of WoW.
    pub hide_when_unfocused: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backend: Backend::default(),
            claude_api_key: String::new(),
            claude_model: CLAUDE_MODELS[0].1.into(),
            google_free_base: "https://translate.googleapis.com".into(),
            claude_base: "https://api.anthropic.com".into(),
            api_base: OLLAMA_BASE.into(),
            api_key: String::new(),
            model: OLLAMA_MODEL.into(),
            server: Region::default(),
            reply_variant: Region::default(),
            reading: Reading::default(),
            translate_names: false,
            overlay_x: 20.0,
            overlay_y: 420.0,
            overlay_width: 520.0,
            overlay_height: 260.0,
            overlay_opacity: 0.55,
            font_size: 15.0,
            max_lines: 100,
            show_original: false,
            anchor_tab: String::new(),
            anchored_opacity: 0.0,
            anchored_font_size: 0.0,
            anchored_width: 0.0,
            anchored_height: 0.0,
            cover_strip: true,
            hide_when_unfocused: true,
        }
    }
}

/// config.json lives next to the executable so the app stays portable. For a macOS app
/// bundle that means next to the `.app`, not inside it, so rebuilding or replacing
/// the app keeps the settings.
pub fn path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| Some(config_dir(&exe)?.join("config.json")))
        .unwrap_or_else(|| PathBuf::from("config.json"))
}

/// The macOS app bundle a program runs from, if it is inside one.
fn app_bundle(exe: &Path) -> Option<&Path> {
    let dir = exe.parent()?;
    dir.parent().and_then(Path::parent).filter(|bundle| {
        dir.ends_with("Contents/MacOS") && bundle.extension().is_some_and(|extension| extension == "app")
    })
}

fn config_dir(exe: &Path) -> Option<&Path> {
    app_bundle(exe).and_then(Path::parent).or(exe.parent())
}

/// Starts a second copy of the companion, for the caller to then quit. An app bundle is
/// opened as one, so the new copy is the same app to macOS and keeps its permissions.
pub fn start_again() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let mut command = match app_bundle(&exe) {
        Some(bundle) => {
            let mut open = std::process::Command::new("open");
            open.arg("-n").arg(bundle);
            open
        }
        None => std::process::Command::new(&exe),
    };
    command.spawn().map(|_| ())
}

pub fn load() -> Config {
    match std::fs::read_to_string(path()) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => {
            let config = Config::default();
            let _ = save(&config);
            config
        }
    }
}

pub fn save(config: &Config) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(config).expect("config serializes");
    std::fs::write(path(), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_config_files_keep_their_endpoint_and_new_installs_need_no_setup() {
        let old: Config = serde_json::from_str(r#"{"api_base":"http://localhost:11434/v1","api_key":"","model":"m"}"#).unwrap();
        assert_eq!(old.backend, Backend::OpenAiCompatible);
        assert_eq!(old.claude_model, "claude-haiku-5-5");
        assert_eq!(Config::default().backend, Backend::GoogleFree);
        let saved = serde_json::to_string(&Config { backend: Backend::Claude, ..Config::default() }).unwrap();
        assert!(saved.contains(r#""backend":"claude""#), "{saved}");
        assert_eq!(serde_json::from_str::<Config>(&saved).unwrap().backend, Backend::Claude);
        assert!(serde_json::to_string(&old).unwrap().contains(r#""backend":"openai_compatible""#));
    }

    #[test]
    fn the_server_and_the_reply_language_are_kept_apart() {
        let old: Config = serde_json::from_str(r#"{"api_base":"http://localhost:1234/v1","model":"m"}"#).unwrap();
        assert_eq!((old.server, old.reply_variant), (Region::Taiwan, Region::Taiwan));
        assert_eq!(old.api_base, "http://localhost:1234/v1");
        // As written before there was a server setting.
        let simplified: Config = serde_json::from_str(r#"{"reply_variant":"zh-Hans"}"#).unwrap();
        assert_eq!((simplified.server, simplified.reply_variant), (Region::Taiwan, Region::Mainland));
        let mixed = Config { server: Region::Mainland, ..Config::default() };
        let saved = serde_json::to_string(&mixed).unwrap();
        assert!(saved.contains(r#""server":"mainland""#) && saved.contains(r#""reply_variant":"taiwan""#), "{saved}");
        let read: Config = serde_json::from_str(&saved).unwrap();
        assert_eq!((read.server, read.reply_variant), (Region::Mainland, Region::Taiwan));
    }

    #[test]
    fn settings_recognise_the_offered_endpoints_and_models() {
        let fresh = Config::default();
        assert_eq!(fresh.endpoint_preset(), Some(2));
        assert_eq!(fresh.claude_preset(), Some(0));
        let openai = Config { api_base: "https://api.openai.com/v1/ ".into(), model: "gpt-4o-mini".into(), ..Config::default() };
        assert_eq!(openai.endpoint_preset(), Some(0));
        let own = Config { model: "qwen2.5:7b".into(), claude_model: "claude-fable-5-2".into(), ..Config::default() };
        assert_eq!(own.endpoint_preset(), None);
        assert_eq!(own.claude_preset(), None);
    }

    #[test]
    fn config_sits_beside_the_program_or_its_app_bundle() {
        let dir = |exe: &str| config_dir(Path::new(exe)).unwrap().to_str().unwrap().to_owned();
        assert_eq!(dir("/opt/nms/translate-companion"), "/opt/nms");
        assert_eq!(
            dir("/opt/nms/Nai Ma Speak.app/Contents/MacOS/translate-companion"),
            "/opt/nms"
        );
        assert_eq!(dir("/opt/Contents/MacOS/translate-companion"), "/opt/Contents/MacOS");
    }
}
