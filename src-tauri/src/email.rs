//! "Draft an email to my sister saying I'll be late": the model writes only a
//! subject + body (`draft_email_text`), then `open_email_draft` opens a new
//! message with them filled in — Gmail's compose page or Apple Mail. The base
//! addresses are fixed here; model text only ever goes into percent-encoded
//! `to` / subject / body fields, so it can't send the browser anywhere else.

use crate::llm::{self, LlmState};
use serde::Serialize;

const GMAIL_COMPOSE: &str = "https://mail.google.com/mail/?view=cm&fs=1";
const MAX_SUBJECT_CHARS: usize = 120;
const MAX_BODY_CHARS: usize = 1500;

#[derive(Serialize, Debug, PartialEq)]
pub struct EmailDraft {
    pub(crate) subject: String,
    pub(crate) body: String,
}

/// Writes a short email for the user's request. The model is asked for
/// "Subject: …" then a blank line then the body — easier for a small model
/// than JSON, and easy to check.
#[tauri::command]
pub async fn draft_email_text(app: tauri::AppHandle, request: String) -> Result<EmailDraft, String> {
    use tauri::Manager;
    let prompt = draft_prompt(&request);
    tauri::async_runtime::spawn_blocking(move || {
        let raw = llm::generate_text(&app.state::<LlmState>(), &prompt, 220)?;
        parse_draft(&raw).ok_or_else(|| "Couldn't write a draft for that.".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens a new message with the draft filled in. `app` is "gmail" or "mail".
#[tauri::command]
pub fn open_email_draft(app: String, to: String, subject: String, body: String) -> Result<(), String> {
    let (cmd_args, target) = match app.as_str() {
        "gmail" => (vec![], gmail_compose_url(&to, &subject, &body)),
        "mail" => (vec!["-b", "com.apple.mail"], mailto_url(&to, &subject, &body)),
        other => return Err(format!("Unknown email app: {other}")),
    };
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open")
            .args(&cmd_args)
            .arg(&target)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Couldn't open the email draft".into());
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (cmd_args, target);
    Ok(())
}

pub(crate) fn draft_prompt(request: &str) -> String {
    format!(
        "Write a short, warm, plain email for this request. Reply in exactly this format and nothing else:\n\
         Subject: <a short subject>\n\n<the email: a greeting, two or three short sentences, and a simple sign-off>\n\n\
         Only say what the request asks — don't make up reasons, details or plans.\n\
         End with a sign-off like \"Love,\" or \"Best,\" and no name after it (the sender adds their own).\n\
         Do not use placeholders like [Name] or [Your name]. Do not add notes or explanations.\n\n\
         Request: {request}"
    )
}

pub(crate) fn parse_draft(raw: &str) -> Option<EmailDraft> {
    let text = raw.replace('\r', "");
    let text = text.trim().trim_start_matches("```").trim_end_matches("```").trim();
    let (first, rest) = text.split_once('\n')?;
    let subject = first
        .trim()
        .trim_start_matches(['*', '#'])
        .trim()
        .strip_prefix("Subject:")
        .or_else(|| first.trim().strip_prefix("subject:"))?
        .trim()
        .trim_matches('*')
        .trim();
    let body: String = rest
        .lines()
        // Drop any placeholder lines the model added anyway ("[Your name]").
        .filter(|l| !(l.trim().starts_with('[') && l.trim().ends_with(']')))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if subject.is_empty() || body.is_empty() {
        return None;
    }
    Some(EmailDraft {
        subject: subject.chars().take(MAX_SUBJECT_CHARS).collect(),
        body: body.chars().take(MAX_BODY_CHARS).collect(),
    })
}

fn gmail_compose_url(to: &str, subject: &str, body: &str) -> String {
    let mut url = GMAIL_COMPOSE.to_string();
    if !to.is_empty() {
        url.push_str(&format!("&to={}", percent_encode(to)));
    }
    url.push_str(&format!("&su={}&body={}", percent_encode(subject), percent_encode(body)));
    url
}

fn mailto_url(to: &str, subject: &str, body: &str) -> String {
    // RFC 6068 line breaks are CRLF.
    let body = body.replace('\n', "\r\n");
    format!("mailto:{}?subject={}&body={}", percent_encode(to), percent_encode(subject), percent_encode(&body))
}

/// Encodes everything except RFC 3986 unreserved characters, so model text
/// can't add parameters or change the address.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_reserved_characters() {
        assert_eq!(percent_encode("Hi & bye?\n#x=1"), "Hi%20%26%20bye%3F%0A%23x%3D1");
        assert_eq!(percent_encode("café"), "caf%C3%A9");
    }

    #[test]
    fn gmail_url_keeps_fixed_base_and_encodes_fields() {
        let url = gmail_compose_url("", "Late tonight", "Hi!\nI'll be late & sorry.");
        assert!(url.starts_with("https://mail.google.com/mail/?view=cm&fs=1&su="));
        assert!(!url.contains("&to="));
        assert!(url.ends_with("&body=Hi%21%0AI%27ll%20be%20late%20%26%20sorry."));
        // A malicious "subject" can't add parameters or change the host.
        let evil = gmail_compose_url("", "x&body=y https://evil.example", "z");
        assert!(evil.starts_with(GMAIL_COMPOSE) && !evil.contains("&body=y"));
    }

    #[test]
    fn mailto_url_uses_crlf() {
        assert_eq!(
            mailto_url("a@b.com", "Hi", "one\ntwo"),
            "mailto:a%40b.com?subject=Hi&body=one%0D%0Atwo"
        );
    }

    #[test]
    fn parses_subject_and_body_and_drops_placeholders() {
        let d = parse_draft("Subject: Running late tonight\n\nHi,\n\nI'll be late this evening.\n\nLove,\n[Your Name]").unwrap();
        assert_eq!(d.subject, "Running late tonight");
        assert_eq!(d.body, "Hi,\n\nI'll be late this evening.\n\nLove,");
        let bold = parse_draft("**Subject:** Dinner\n\nSee you at 7.").unwrap();
        assert_eq!(bold.subject, "Dinner");
    }

    #[test]
    fn rejects_drafts_without_subject_or_body() {
        assert_eq!(parse_draft("Hi, I'll be late."), None);
        assert_eq!(parse_draft("Subject: Hello\n\n"), None);
    }
}
