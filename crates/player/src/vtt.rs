use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

fn timestamp(value: &str) -> Option<f64> {
    let values: Vec<_> = value.split(':').collect();
    let (hours, minutes, seconds) = match values.as_slice() {
        [m, s] => (0.0, m.parse::<f64>().ok()?, s.parse::<f64>().ok()?),
        [h, m, s] => (
            h.parse::<f64>().ok()?,
            m.parse::<f64>().ok()?,
            s.parse::<f64>().ok()?,
        ),
        _ => return None,
    };
    let time = hours * 3600.0 + minutes * 60.0 + seconds;
    (time.is_finite()
        && hours >= 0.0
        && (0.0..60.0).contains(&minutes)
        && (0.0..60.0).contains(&seconds))
    .then_some(time)
}
fn plain_text(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for ch in value.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&lrm;", "\u{200e}")
        .replace("&rlm;", "\u{200f}")
        .replace("&amp;", "&")
}
pub fn parse(input: &str) -> Result<Vec<Cue>> {
    let input = input
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let header = input.lines().next().unwrap_or("");
    if !(header == "WEBVTT" || header.starts_with("WEBVTT ") || header.starts_with("WEBVTT\t")) {
        return Err(Error::InvalidSubtitle);
    }
    let mut cues = Vec::new();
    for block in input.split("\n\n").skip(1) {
        let mut lines = block.lines();
        let Some(first) = lines.next() else { continue };
        if first.starts_with("NOTE") || first == "STYLE" || first == "REGION" {
            continue;
        }
        let timing = if first.contains("-->") {
            first
        } else {
            lines.next().unwrap_or("")
        };
        let Some((start, end)) = timing.split_once("-->") else {
            continue;
        };
        let Some(start) = timestamp(start.trim()) else {
            return Err(Error::InvalidSubtitle);
        };
        let Some(end) = end.split_whitespace().next().and_then(timestamp) else {
            return Err(Error::InvalidSubtitle);
        };
        if end <= start {
            return Err(Error::InvalidSubtitle);
        }
        let text = plain_text(&lines.collect::<Vec<_>>().join("\n"));
        if !text.trim().is_empty() {
            cues.push(Cue { start, end, text });
        }
        if cues.len() > 100_000 {
            return Err(Error::InvalidSubtitle);
        }
    }
    cues.sort_by(|a, b| a.start.total_cmp(&b.start));
    Ok(cues)
}
