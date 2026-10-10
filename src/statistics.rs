//! Listening history uses rendered audio time, never the seek position.
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct ListeningRecord {
    pub session_id: String,
    pub track_id: i64,
    pub duration_ms: u64,
    pub started_at_ms: i64,
    pub updated_at_ms: i64,
    pub listened_ms: u64,
    pub qualified: bool,
    pub ended_at_ms: Option<i64>,
}

pub fn listening_threshold(duration_ms: u64) -> u64 {
    if duration_ms == 0 {
        240_000
    } else {
        duration_ms.div_ceil(2).min(240_000)
    }
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[derive(Default)]
pub(crate) struct ListeningTracker {
    active: Option<ListeningRecord>,
}
impl ListeningTracker {
    pub(crate) fn begin(&mut self, track_id: i64, duration_ms: u64) {
        let now = now_ms();
        self.active = Some(ListeningRecord {
            session_id: format!("{now}-{:016x}{:016x}", fastrand::u64(..), fastrand::u64(..)),
            track_id,
            duration_ms,
            started_at_ms: now,
            updated_at_ms: now,
            listened_ms: 0,
            qualified: false,
            ended_at_ms: None,
        });
    }
    pub(crate) fn observe(&mut self, elapsed: Duration) -> bool {
        let Some(record) = &mut self.active else {
            return false;
        };
        let was_qualified = record.qualified;
        let elapsed = elapsed.as_millis().min(i64::MAX as u128) as u64;
        if elapsed > record.listened_ms {
            record.listened_ms = elapsed;
            record.updated_at_ms = now_ms();
            record.qualified |= elapsed >= listening_threshold(record.duration_ms);
        }
        !was_qualified && record.qualified
    }
    pub(crate) fn checkpoint(&self) -> Option<ListeningRecord> {
        self.active
            .as_ref()
            .filter(|record| record.listened_ms > 0)
            .cloned()
    }
    pub(crate) fn finish(&mut self) -> Option<ListeningRecord> {
        let mut record = self.active.take()?;
        if record.listened_ms == 0 {
            return None;
        }
        record.ended_at_ms = Some(now_ms());
        Some(record)
    }
}

#[derive(Clone, Debug, Default)]
pub struct StatisticsSummary {
    pub listened_ms: u64,
    pub plays: u64,
    pub starts: u64,
    pub tracks: usize,
    pub artists: usize,
    pub albums: usize,
}

#[derive(Clone, Debug, Default)]
pub struct StatisticsRow {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub plays: u64,
    pub starts: u64,
    pub listened_ms: u64,
    pub tracks: usize,
    pub last_at_ms: i64,
    pub date: String,
}

#[derive(Clone, Debug, Default)]
pub struct ListeningStatistics {
    pub summary: StatisticsSummary,
    pub tracks: Vec<StatisticsRow>,
    pub artists: Vec<StatisticsRow>,
    pub albums: Vec<StatisticsRow>,
    pub recent: Vec<StatisticsRow>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn threshold_uses_half_duration_or_four_minutes_and_handles_unknown_duration() {
        assert_eq!(listening_threshold(180_001), 90_001);
        assert_eq!(listening_threshold(900_000), 240_000);
        assert_eq!(listening_threshold(0), 240_000);
        assert_eq!(listening_threshold(1), 1);
    }
    #[test]
    fn partials_rewinds_and_repeats_are_distinct_and_qualified_once() {
        let mut tracker = ListeningTracker::default();
        tracker.begin(7, 180_000);
        assert!(tracker.checkpoint().is_none());
        assert!(!tracker.observe(Duration::from_secs(20)));
        assert!(!tracker.observe(Duration::from_secs(20))); // Pause/seek does not advance PCM time.
        assert_eq!(tracker.finish().unwrap().listened_ms, 20_000);
        tracker.begin(7, 180_000);
        let first_id = tracker.checkpoint();
        assert!(first_id.is_none());
        assert!(tracker.observe(Duration::from_secs(90)));
        let first = tracker.checkpoint().unwrap();
        assert!(!tracker.observe(Duration::from_secs(120)));
        assert!(!tracker.observe(Duration::from_secs(0))); // Clock cannot decrease stored time.
        let finished = tracker.finish().unwrap();
        assert!(finished.qualified);
        assert_eq!(finished.listened_ms, 120_000);
        assert!(finished.ended_at_ms.is_some());
        tracker.begin(7, 180_000);
        tracker.observe(Duration::from_secs(90));
        assert_ne!(first.session_id, tracker.finish().unwrap().session_id);
    }
}
