//! Claude through Anthropic's Messages API, with a Claude Console API key.

use crate::config::Config;
use crate::translate::{region, system_prompt, Task};
use serde_json::{json, Value};
use std::thread;
use std::time::Duration;

const API_VERSION: &str = "2026-10-21";
/// How many times a request is sent again after a rate-limit or overload answer.
const RETRIES: u32 = 2;
/// Longest time to wait before sending again, whatever the service asks for.
const LONGEST_WAIT: Duration = Duration::from_secs(20);

/// Why a key cannot be used, if it cannot. Keys from the Claude Code or claude.ai login
/// (sk-ant-oat...) are for Anthropic's own apps only; this app needs a Console API key.
pub fn key_problem(key: &str) -> Option<&'static str> {
    let key = key.trim();
    if key.is_empty() {
        Some("Enter a Claude Console API key (console.anthropic.com, API keys).")
    } else if key.starts_with("sk-ant-oat") {
        Some("This is a Claude subscription login token, which only Anthropic's own apps may use. Create an API key in the Claude Console instead.")
    } else {
        None
    }
}

/// The request body. No sampling settings: API version 2026-10-21 rejects temperature,
/// top_p and top_k. A chat line needs no reasoning, so thinking is turned off on Haiku,
/// the models that allow it; other models only take a lower effort.
fn request_body(config: &Config, text: &str, task: Task) -> Value {
    let model = config.claude_model.trim();
    let mut body = json!({
        "model": model,
        "system": system_prompt(text, task, region(config, task)),
        "messages": [{ "role": "user", "content": crate::translate::user_message(text, task) }],
    });
    if model.to_lowercase().contains("haiku") {
        // Room for the word-by-word explanation of a long line.
        body["max_tokens"] = json!(2048);
        body["thinking"] = json!({ "type": "disabled" });
    } else {
        // Thinking cannot be turned off here and shares the limit with the answer.
        body["max_tokens"] = json!(8192);
        body["output_config"] = json!({ "effort": "low" });
    }
    body
}

/// Reads the translation: the first text block of the answer. Says so when Claude
/// declined the line.
fn parse(answer: &Value) -> Result<String, String> {
    if answer["stop_reason"] == "refusal" {
        let category = answer["stop_details"]["category"].as_str().unwrap_or("unspecified");
        return Err(format!("Claude declined to translate this line (safety category: {category})"));
    }
    let blocks = answer["content"].as_array().map(Vec::as_slice).unwrap_or_default();
    let text = blocks.iter().find(|block| block["type"] == "text").and_then(|block| block["text"].as_str());
    text.map(str::to_string).ok_or_else(|| {
        let detail: String = answer.to_string().chars().take(200).collect();
        format!("Claude gave no translation: {detail}")
    })
}

pub fn translate(config: &Config, text: &str, task: Task, notify: &dyn Fn(String)) -> Result<String, String> {
    if let Some(problem) = key_problem(&config.claude_api_key) {
        return Err(problem.to_string());
    }
    let url = format!("{}/v1/messages", config.claude_base.trim().trim_end_matches('/'));
    let body = request_body(config, text, task);

    for attempt in 0..=RETRIES {
        let request = ureq::post(&url)
            .timeout(Duration::from_secs(60))
            .set("x-api-key", config.claude_api_key.trim())
            .set("anthropic-version", API_VERSION);
        match request.send_json(&body) {
            Ok(response) => {
                let answer: Value = response.into_json().map_err(|e| format!("bad JSON from Claude: {e}"))?;
                return parse(&answer);
            }
            // 429: rate limit. 529: the service is overloaded. Both pass.
            Err(ureq::Error::Status(code @ (429 | 529), response)) if attempt < RETRIES => {
                let asked = response.header("retry-after").and_then(|seconds| seconds.trim().parse::<f32>().ok());
                let wait = Duration::from_secs_f32(asked.unwrap_or(2.0).clamp(0.0, LONGEST_WAIT.as_secs_f32()));
                let why = if code == 429 { "rate-limiting" } else { "overloaded" };
                notify(format!("Claude is {why}; waiting {:.0} s", wait.as_secs_f32().ceil()));
                thread::sleep(wait);
            }
            Err(ureq::Error::Status(code, response)) => {
                let answer: Value = response.into_json().unwrap_or_default();
                let message = answer["error"]["message"].as_str().unwrap_or("no details given");
                return Err(match code {
                    401 => format!("Claude rejected the API key: {message}"),
                    _ => format!("Claude returned HTTP {code}: {}", message.chars().take(200).collect::<String>()),
                });
            }
            Err(e) => return Err(format!("could not reach Claude: {e}")),
        }
    }
    unreachable!("the last attempt returns either way")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::mock_server;
    use std::cell::RefCell;

    const ANSWER: &str = r#"{"type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"thinking","thinking":""},{"type":"text","text":"Need a healer, whisper me"}]}"#;

    fn config(base: String) -> Config {
        Config { claude_base: base, claude_api_key: "sk-ant-api03-test".into(), ..Config::default() }
    }

    #[test]
    fn sends_a_messages_request() {
        let (base, server) = mock_server(vec![(200, "", ANSWER)]);
        let english = translate(&config(base), "副本缺補 密我", Task::ToEnglish, &|_| ()).unwrap();
        assert_eq!(english, "Need a healer, whisper me", "the thinking block is skipped");

        let request = &server.join().unwrap()[0];
        let (head, body) = request.split_once("\r\n\r\n").unwrap();
        let head = head.to_lowercase();
        assert!(head.starts_with("post /v1/messages "), "{head}");
        assert!(head.contains("x-api-key: sk-ant-api03-test"), "{head}");
        assert!(head.contains("anthropic-version: 2026-10-21"), "{head}");
        assert!(!head.contains("authorization"), "{head}");

        let body: Value = serde_json::from_str(body).unwrap();
        assert_eq!(body["model"], "claude-haiku-5-5");
        assert_eq!(body["thinking"]["type"], "disabled");
        assert_eq!(body["messages"], json!([{ "role": "user", "content": "<text>副本缺補 密我</text>" }]));
        assert!(body["system"].as_str().unwrap().contains("never addressed to you"), "told to translate, not answer");
        assert!(body["system"].as_str().unwrap().contains("Slang and places in this line: 副本 = dungeon"));
        for rejected in ["temperature", "top_p", "top_k"] {
            assert!(body.get(rejected).is_none(), "{rejected} must not be sent");
        }
    }

    #[test]
    fn only_haiku_has_thinking_turned_off() {
        let other = Config { claude_model: "claude-opus-5-5".into(), ..Config::default() };
        let body = request_body(&other, "缺補", Task::ToEnglish);
        assert!(body.get("thinking").is_none());
        assert_eq!(body["output_config"]["effort"], "low");
        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn subscription_tokens_and_missing_keys_are_refused_before_any_request() {
        assert!(key_problem("sk-ant-oat01-abc").unwrap().contains("subscription"));
        assert!(key_problem("  ").is_some());
        assert_eq!(key_problem("sk-ant-api03-abc"), None);
        // An unroutable base: a request would fail differently, and slowly.
        let config = Config { claude_api_key: "sk-ant-oat01-abc".into(), claude_base: "http://127.0.0.1:1".into(), ..Config::default() };
        let error = translate(&config, "缺補", Task::ToEnglish, &|_| ()).unwrap_err();
        assert!(error.contains("subscription login token"), "{error}");
    }

    #[test]
    fn waits_as_asked_and_sends_again() {
        let limited = r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#;
        let (base, server) = mock_server(vec![(429, "retry-after: 0\r\n", limited), (529, "", limited), (200, "", ANSWER)]);
        let notes = RefCell::new(Vec::new());
        // The 529 carries no retry-after, so this waits the default 2 s once.
        let english = translate(&config(base), "缺補", Task::ToEnglish, &|note| notes.borrow_mut().push(note)).unwrap();
        assert_eq!(english, "Need a healer, whisper me");
        assert_eq!(server.join().unwrap().len(), 3);
        let notes = notes.into_inner();
        assert!(notes[0].contains("rate-limiting") && notes[1].contains("overloaded"), "{notes:?}");
    }

    #[test]
    fn reports_a_wrong_key_and_a_declined_line_clearly() {
        let wrong = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        let (base, server) = mock_server(vec![(401, "", wrong)]);
        let error = translate(&config(base), "缺補", Task::ToEnglish, &|_| ()).unwrap_err();
        assert_eq!(error, "Claude rejected the API key: invalid x-api-key");
        assert_eq!(server.join().unwrap().len(), 1, "a wrong key is not retried");

        let declined: Value =
            serde_json::from_str(r#"{"stop_reason":"refusal","stop_details":{"type":"refusal","category":"general_harms"},"content":[]}"#).unwrap();
        assert!(parse(&declined).unwrap_err().contains("declined"));
        let empty: Value = serde_json::from_str(r#"{"stop_reason":"end_turn","content":[]}"#).unwrap();
        assert!(parse(&empty).unwrap_err().contains("no translation"));
    }
}
