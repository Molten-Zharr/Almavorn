use std::ops::Range;

#[derive(Clone, Debug)]
struct WordTiming {
    text: Range<usize>,
    start: u64,
    end: u64,
}

#[derive(Clone, Debug)]
pub struct LyricLine {
    pub time: Option<u64>,
    pub text: String,
    words: Vec<WordTiming>,
}

impl LyricLine {
    pub(crate) fn word_at(&self, position: u64) -> Option<Range<usize>> {
        self.words
            .iter()
            .find(|word| word.start <= position && position < word.end)
            .map(|word| word.text.clone())
    }
}

fn timestamp(value: &str) -> Option<u64> {
    let (minutes, seconds) = value.split_once(':')?;
    let (seconds, fraction) = seconds
        .split_once('.')
        .map_or((seconds, None), |(s, f)| (s, Some(f)));
    if [minutes, seconds]
        .iter()
        .any(|part| part.is_empty() || !part.bytes().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    let minutes: u64 = minutes.parse().ok()?;
    let seconds: u64 = seconds.parse().ok()?;
    if seconds >= 60 {
        return None;
    }
    let millis = if let Some(fraction) = fraction {
        if fraction.is_empty()
            || fraction.len() > 9
            || !fraction.bytes().all(|c| c.is_ascii_digit())
        {
            return None;
        }
        fraction
            .bytes()
            .chain(std::iter::repeat(b'0'))
            .take(3)
            .fold(0u64, |value, digit| value * 10 + u64::from(digit - b'0'))
    } else {
        0
    };
    minutes
        .checked_mul(60_000)?
        .checked_add(seconds * 1000)?
        .checked_add(millis)
}

fn inline_words(raw: &str) -> (String, Vec<WordTiming>) {
    let mut text = String::new();
    let mut marks = Vec::new();
    let mut rest = raw;
    let mut valid = true;
    while let Some(start) = rest.find('<') {
        text.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find('>') else {
            valid = false;
            break;
        };
        if let Some(time) = timestamp(&rest[1..end]) {
            marks.push((time, text.len()));
        } else {
            valid = false;
            text.push_str(&rest[..=end]);
        }
        rest = &rest[end + 1..];
    }
    text.push_str(rest);
    let leading = text.len() - text.trim_start().len();
    let trailing = text.trim_end().len();
    let mut words = Vec::new();
    if valid && marks.windows(2).all(|pair| pair[0].0 < pair[1].0) {
        // Both boundaries come from explicit inline markers. In particular,
        // never invent the last word's end from the next line or song duration.
        for pair in marks.windows(2) {
            let ((start, from), (end, to)) = (pair[0], pair[1]);
            let segment = &text[from..to];
            let word = segment.trim();
            if word.is_empty() || word.split_whitespace().count() != 1 {
                continue;
            }
            let from = from + segment.len() - segment.trim_start().len();
            let to = from + word.len();
            if from >= leading && to <= trailing {
                words.push(WordTiming {
                    text: from - leading..to - leading,
                    start,
                    end,
                });
            }
        }
    }
    (text.trim().to_owned(), words)
}

pub(super) fn lines(content: &str) -> Vec<LyricLine> {
    // Offset extensions are not interpreted by the line reader. Do not offer
    // word precision for a timeline whose offset we cannot currently apply.
    let allow_words = !content.lines().any(|line| {
        line.trim().strip_prefix("[offset:").is_some_and(|value| {
            value
                .strip_suffix(']')
                .and_then(|v| v.trim().parse::<i64>().ok())
                != Some(0)
        })
    });
    let mut lines = Vec::new();
    for raw in content.lines() {
        let mut rest = raw;
        let mut times = Vec::new();
        while let Some(tail) = rest.strip_prefix('[') {
            let Some(end) = tail.find(']') else {
                break;
            };
            let Some(time) = timestamp(&tail[..end]) else {
                break;
            };
            times.push(time);
            rest = &tail[end + 1..];
        }
        if times.is_empty() {
            lines.push(LyricLine {
                time: None,
                text: raw.to_owned(),
                words: Vec::new(),
            });
        } else {
            let (text, words) = inline_words(rest.trim());
            for time in times {
                let words = if allow_words && words.iter().all(|word| word.start >= time) {
                    words.clone()
                } else {
                    Vec::new()
                };
                lines.push(LyricLine {
                    time: Some(time),
                    text: text.clone(),
                    words,
                });
            }
        }
    }
    let times: std::collections::BTreeSet<_> = lines.iter().filter_map(|line| line.time).collect();
    for line in &mut lines {
        if let Some(start) = line.time
            && let Some(end) = times
                .range((std::ops::Bound::Excluded(start), std::ops::Bound::Unbounded))
                .next()
            && line.words.iter().any(|word| word.end > *end)
        {
            line.words.clear();
        }
    }
    lines
}
