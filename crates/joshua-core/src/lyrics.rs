//! Song lyrics, kept next to the song (`lyrics.json`) so they show offline
//! once found. Time-synced lyrics come as LRC text (`[01:02.50] line`);
//! [`parse_lrc`] turns it into lines on the song timeline.

use serde::{Deserialize, Serialize};

/// Longest lyrics accepted (bytes of text), far above any real song.
pub const MAX_LYRICS_BYTES: usize = 64 * 1024;

/// One line of synced lyrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricLine {
    /// Song time (seconds) the line starts.
    pub secs: f64,
    pub text: String,
}

/// What is stored in `lyrics.json`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    /// Where they came from (`lrclib`), shown as the credit.
    pub source: String,
    /// Time-synced lines, in order; empty when only plain text is known.
    pub synced: Vec<LyricLine>,
    /// The whole text, unsynced.
    pub plain: String,
    /// The song is known to have no vocals.
    #[serde(default)]
    pub instrumental: bool,
    /// Moves every synced line (seconds), when the recording found online is
    /// a slightly different edit than the user's file.
    #[serde(default)]
    pub offset_secs: f64,
}

impl Lyrics {
    /// Nothing to show.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.synced.is_empty() && self.plain.trim().is_empty()
    }
}

/// Parses LRC text: `[mm:ss.xx]` stamps (several per line allowed), the
/// `[offset:±ms]` tag; other tags (`[ar:…]`) and unstamped lines are skipped.
/// Lines come back sorted by time.
#[must_use]
pub fn parse_lrc(text: &str) -> Vec<LyricLine> {
    let mut offset = 0.0;
    let mut lines = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        while let Some(tail) = rest.strip_prefix('[') {
            let Some((tag, after)) = tail.split_once(']') else { break };
            if let Some(ms) = tag.strip_prefix("offset:") {
                // Positive offset: lyrics appear sooner.
                offset = ms.trim().parse::<f64>().map_or(0.0, |ms| -ms / 1000.0);
            } else if let Some(t) = parse_stamp(tag) {
                stamps.push(t);
            }
            rest = after.trim_start();
        }
        let text: String = rest.trim().chars().filter(|c| !c.is_control()).collect();
        for t in stamps {
            lines.push(LyricLine { secs: t, text: text.clone() });
        }
    }
    for l in &mut lines {
        l.secs = (l.secs + offset).max(0.0);
    }
    lines.sort_by(|a, b| a.secs.total_cmp(&b.secs));
    lines
}

/// `mm:ss`, `mm:ss.x`, `mm:ss.xx`, `mm:ss.xxx` or `mm:ss:xx`.
fn parse_stamp(tag: &str) -> Option<f64> {
    let (min, sec) = tag.split_once(':')?;
    let min: u32 = min.trim().parse().ok()?;
    let sec = sec.replacen(':', ".", 1);
    if !sec.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let sec: f64 = sec.parse().ok()?;
    (sec < 60.0).then(|| f64::from(min) * 60.0 + sec)
}

/// Index of the line sung at `secs` (the last one started), if any.
#[must_use]
pub fn line_at(lines: &[LyricLine], secs: f64) -> Option<usize> {
    lines.partition_point(|l| l.secs <= secs).checked_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lrc_lines_land_on_the_timeline() {
        let lines = parse_lrc("[ar:Someone]\n[00:12.15] I'm fired up\n[01:02.5]Second\n\n[00:00.00] \n");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], LyricLine { secs: 0.0, text: String::new() });
        assert!((lines[1].secs - 12.15).abs() < 1e-9);
        assert_eq!(lines[1].text, "I'm fired up");
        assert!((lines[2].secs - 62.5).abs() < 1e-9);
    }

    #[test]
    fn repeated_stamps_and_the_offset_tag() {
        let lines = parse_lrc("[offset:+500]\n[00:20.00][00:10.00]Chorus\n[00:15.000]Verse");
        let got: Vec<(f64, &str)> = lines.iter().map(|l| (l.secs, l.text.as_str())).collect();
        assert_eq!(got, vec![(9.5, "Chorus"), (14.5, "Verse"), (19.5, "Chorus")]);
    }

    #[test]
    fn junk_is_ignored() {
        assert!(parse_lrc("no stamps here\n[xx:yy]bad\n[00:75.00]bad seconds").is_empty());
        assert!(parse_lrc("[00:01.00 unterminated").is_empty());
    }

    #[test]
    fn the_current_line_is_the_last_one_started() {
        let lines = parse_lrc("[00:05.00]a\n[00:10.00]b");
        assert_eq!(line_at(&lines, 1.0), None);
        assert_eq!(line_at(&lines, 5.0), Some(0));
        assert_eq!(line_at(&lines, 9.9), Some(0));
        assert_eq!(line_at(&lines, 60.0), Some(1));
    }
}
