//! Google Translate's free web endpoint: no key and no setup, but unofficial. Google may
//! rate-limit it or change it without notice, so every failure says what to switch to.
//! It is plain machine translation: it takes no instructions and no slang hints.

use crate::config::{Config, Region};
use crate::translate::Task;
use serde_json::Value;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

/// Shortest time between two requests.
const SPACING: Duration = Duration::from_millis(300);
/// The wait after a rate-limit answer doubles from one second up to this.
const LONGEST_WAIT: Duration = Duration::from_secs(30);
/// How many times one line is tried before it is given up: with the doubling wait, a
/// line outlasts about half a minute of rate-limiting.
const ATTEMPTS: u32 = 6;

/// When the next request may go out, and the wait to impose after the next rate-limit
/// answer. Shared by all lines, so a limit met by one slows the ones after it too.
struct Pace {
    next: Option<Instant>,
    wait: Duration,
}

static PACE: Mutex<Pace> = Mutex::new(Pace { next: None, wait: Duration::from_secs(1) });

/// Reads the translation out of the endpoint's answer, nested arrays in which each
/// `[0][i][0]` is one translated stretch of the text.
fn parse(answer: &Value) -> Option<String> {
    let parts = answer.get(0)?.as_array()?;
    let text: String = parts.iter().filter_map(|part| part.get(0)?.as_str()).collect();
    (!text.is_empty()).then_some(text)
}

pub fn translate(config: &Config, text: &str, task: Task, notify: &dyn Fn(String)) -> Result<String, String> {
    let url = format!("{}/translate_a/single", config.google_free_base.trim().trim_end_matches('/'));
    let (source, target) = match (task, config.reply_variant) {
        (Task::ToEnglish, _) => ("auto", "en"),
        (Task::ToChinese | Task::ToChineseAgain, Region::Taiwan) => ("en", "zh-TW"),
        (Task::ToChinese | Task::ToChineseAgain, Region::Mainland) => ("en", "zh-CN"),
        (Task::Explain(_) | Task::Name | Task::ExplainName, _) => {
            return Err("Google Translate cannot explain lines or names. Choose Claude or an OpenAI-compatible model on the Backend tab.".into())
        }
    };
    let advice = "Try again later, or pick another backend in Settings.";

    for attempt in 1..=ATTEMPTS {
        let due = PACE.lock().unwrap().next;
        if let Some(wait) = due.and_then(|due| due.checked_duration_since(Instant::now())) {
            thread::sleep(wait);
        }
        let request = ureq::get(&url)
            .timeout(Duration::from_secs(20))
            .query("client", "gtx")
            .query("sl", source)
            .query("tl", target)
            .query("dt", "t")
            .query("q", text);
        let result = request.call();
        let mut pace = PACE.lock().unwrap();
        match result {
            Ok(response) => {
                pace.next = Some(Instant::now() + SPACING);
                pace.wait = Duration::from_secs(1);
                drop(pace);
                let answer: Value = response.into_json().map_err(|e| format!("Google Translate sent an answer that is not JSON: {e}"))?;
                return parse(&answer).ok_or_else(|| {
                    let detail: String = answer.to_string().chars().take(120).collect();
                    format!("Google Translate's free service answered in a form this app does not know: {detail}. {advice}")
                });
            }
            Err(ureq::Error::Status(code @ (429 | 503), _)) => {
                let wait = pace.wait;
                pace.next = Some(Instant::now() + wait);
                pace.wait = (wait * 2).min(LONGEST_WAIT);
                drop(pace);
                if attempt < ATTEMPTS {
                    notify(format!("Google Translate is rate-limiting (HTTP {code}); waiting {} s", wait.as_secs()));
                }
            }
            Err(ureq::Error::Status(code, _)) => {
                pace.next = Some(Instant::now() + SPACING);
                return Err(format!("Google Translate's free service returned HTTP {code}. {advice}"));
            }
            Err(e) => {
                pace.next = Some(Instant::now() + SPACING);
                return Err(format!("could not reach Google Translate: {e}"));
            }
        }
    }
    Err(format!("Google Translate's free service keeps rate-limiting. {advice}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::mock_server;
    use std::cell::RefCell;

    fn config(base: String) -> Config {
        Config { google_free_base: base, ..Config::default() }
    }

    #[test]
    fn joins_the_translated_stretches() {
        let one: Value = serde_json::from_str(r#"[[["Need a healer","缺補",null,null,10]],null,"zh-TW"]"#).unwrap();
        assert_eq!(parse(&one).as_deref(), Some("Need a healer"));
        let two: Value =
            serde_json::from_str(r#"[[["Need a healer. ","缺補。",null,null,10],["Whisper me","密我",null,null,3]],null,"zh-TW"]"#).unwrap();
        assert_eq!(parse(&two).as_deref(), Some("Need a healer. Whisper me"));
        assert_eq!(parse(&serde_json::json!({"error": "no"})), None);
        assert_eq!(parse(&serde_json::json!([[]])), None);
    }

    #[test]
    fn asks_google_without_a_key() {
        let (base, server) = mock_server(vec![(200, "", r#"[[["Need a healer","缺補",null,null,10]],null,"zh-TW"]"#)]);
        let english = translate(&config(base), "缺補 密我", Task::ToEnglish, &|_| ()).unwrap();
        assert_eq!(english, "Need a healer");
        let request = &server.join().unwrap()[0];
        let line = request.lines().next().unwrap();
        assert!(line.starts_with("GET /translate_a/single?"), "{line}");
        for wanted in ["client=gtx", "sl=auto", "tl=en", "dt=t", "q=%E7%BC%BA%E8%A3%9C+%E5%AF%86%E6%88%91"] {
            assert!(line.contains(wanted), "{wanted} missing from {line}");
        }
        assert!(!request.to_lowercase().contains("authorization"));
    }

    #[test]
    fn replies_ask_for_the_chosen_chinese() {
        let reply = r#"[[["需要治療","need a healer",null,null,10]],null,"en"]"#;
        let (base, server) = mock_server(vec![(200, "", reply)]);
        translate(&config(base), "need a healer", Task::ToChinese, &|_| ()).unwrap();
        let request = &server.join().unwrap()[0];
        assert!(request.contains("sl=en") && request.contains("tl=zh-TW"), "{request}");

        let (base, server) = mock_server(vec![(200, "", reply)]);
        let simplified = Config { reply_variant: Region::Mainland, ..config(base) };
        translate(&simplified, "need a healer", Task::ToChinese, &|_| ()).unwrap();
        assert!(server.join().unwrap()[0].contains("tl=zh-CN"));
    }

    #[test]
    fn waits_and_tries_again_when_rate_limited() {
        let ok = r#"[[["Need a tank","缺坦",null,null,10]],null,"zh-TW"]"#;
        let (base, server) = mock_server(vec![(429, "", "Too Many Requests"), (200, "", ok)]);
        let notes = RefCell::new(Vec::new());
        let english = translate(&config(base), "缺坦", Task::ToEnglish, &|note| notes.borrow_mut().push(note)).unwrap();
        assert_eq!(english, "Need a tank");
        assert_eq!(server.join().unwrap().len(), 2);
        let notes = notes.into_inner();
        assert_eq!(notes.len(), 1, "one note for the wait, and no failure: {notes:?}");
        assert!(notes[0].contains("rate-limiting"), "{notes:?}");
    }

    #[test]
    fn other_errors_fail_at_once_with_advice() {
        let (base, server) = mock_server(vec![(404, "", "gone")]);
        let error = translate(&config(base), "缺坦", Task::ToEnglish, &|_| ()).unwrap_err();
        assert!(error.contains("HTTP 404") && error.contains("another backend"), "{error}");
        assert_eq!(server.join().unwrap().len(), 1);
    }
}
