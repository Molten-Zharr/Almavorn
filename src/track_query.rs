//! Literal substring search shared by the playlist filter and global search.
use crate::model::Track;

/// Normalized searchable fields, prepared once for the global search index.
pub(crate) struct TrackText(String);

impl TrackText {
    pub fn new(track: &Track) -> Self {
        Self(
            format!(
                "{} {} {} {}",
                track.title,
                track.artist,
                track.album,
                track.path.display()
            )
            .to_lowercase(),
        )
    }
}

/// Every whitespace-separated term must appear in at least one searchable field.
/// Unicode lowercasing preserves accents, punctuation and literal query characters.
pub(crate) struct TrackQuery {
    terms: Vec<String>,
}

impl TrackQuery {
    pub fn new(query: &str) -> Self {
        Self {
            terms: query
                .to_lowercase()
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
        }
    }

    pub fn matches(&self, text: &TrackText) -> bool {
        self.terms.iter().all(|term| text.0.contains(term))
    }

    pub fn matches_track(&self, track: &Track) -> bool {
        // An empty filter avoids formatting and normalizing every library row.
        self.terms.is_empty() || self.matches(&TrackText::new(track))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_terms_do_not_join_across_fields_and_non_searchable_metadata_stays_excluded() {
        let track = Track {
            id: 991,
            title: "Bon".into(),
            artist: "Obo".into(),
            album: String::new(),
            path: "/Music/track.wav".into(),
            duration_ms: 123456,
            tags: "hidden-tag".into(),
        };
        assert!(TrackQuery::new("BON obo").matches_track(&track));
        for query in ["bonobo", "hidden-tag", "123456", "991"] {
            assert!(!TrackQuery::new(query).matches_track(&track), "{query}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_paths_keep_their_display_representation_searchable() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let track = Track {
            id: 1,
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            path: OsString::from_vec(b"/Music/NEON\xff.WAV".to_vec()).into(),
            duration_ms: 0,
            tags: String::new(),
        };
        assert!(TrackQuery::new("music neon\u{fffd}.wav").matches_track(&track));
    }
}
