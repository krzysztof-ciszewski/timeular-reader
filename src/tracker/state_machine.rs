use chrono::{DateTime, Local};

use super::config::{Side, TimeEntry};

#[derive(Debug, Default)]
pub struct Tracker {
    current: Option<(Side, DateTime<Local>)>,
}

impl Tracker {
    pub fn on_side(&mut self, side: Option<&Side>, now: DateTime<Local>) -> Option<TimeEntry> {
        let is_same_side = match (self.current.as_ref(), side) {
            (Some((current_side, _)), Some(side)) => current_side.side_num == side.side_num,
            _ => false,
        };

        if is_same_side {
            return None;
        }

        let completed = self.current.take().map(|(previous_side, start)| TimeEntry {
            side: previous_side,
            start,
            end: now,
        });

        if let Some(side) = side.filter(|side| side.is_trackable()) {
            self.current = Some((side.clone(), now));
        }

        completed
    }

    pub fn finish(&mut self, now: DateTime<Local>) -> Option<TimeEntry> {
        self.current.take().map(|(side, start)| TimeEntry {
            side,
            start,
            end: now,
        })
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Local, TimeZone};

    use super::Tracker;
    use crate::tracker::config::{Side, TimeEntry};

    fn side(side_num: u8, label: &str) -> Side {
        Side {
            side_num,
            label: label.to_string(),
            configurable: true,
        }
    }

    fn at(timestamp: i64) -> chrono::DateTime<Local> {
        Local.timestamp_opt(timestamp, 0).single().unwrap()
    }

    #[test]
    fn repeated_notifications_do_not_reset_the_start_time() {
        let work = side(1, "Work");
        let mut tracker = Tracker::default();

        assert_eq!(tracker.on_side(Some(&work), at(10)), None);
        assert_eq!(tracker.on_side(Some(&work), at(20)), None);
        assert_eq!(
            tracker.on_side(Some(&side(2, "Break")), at(30)),
            Some(TimeEntry {
                side: work,
                start: at(10),
                end: at(30),
            })
        );
    }

    #[test]
    fn moving_to_an_untrackable_side_completes_and_clears_entry() {
        let work = side(1, "Work");
        let unconfigured = side(9, "");
        let mut tracker = Tracker::default();

        tracker.on_side(Some(&work), at(10));

        assert_eq!(
            tracker.on_side(Some(&unconfigured), at(20)),
            Some(TimeEntry {
                side: work,
                start: at(10),
                end: at(20),
            })
        );
        assert_eq!(tracker.on_side(Some(&unconfigured), at(30)), None);
        assert_eq!(tracker.on_side(Some(&side(2, "Break")), at(40)), None);
    }

    #[test]
    fn moving_between_trackable_sides_starts_the_new_entry_at_the_transition() {
        let work = side(1, "Work");
        let break_side = side(2, "Break");
        let mut tracker = Tracker::default();

        tracker.on_side(Some(&work), at(10));
        let completed = tracker.on_side(Some(&break_side), at(20));

        assert_eq!(
            completed,
            Some(TimeEntry {
                side: work,
                start: at(10),
                end: at(20),
            })
        );
        assert_eq!(
            tracker.on_side(Some(&side(3, "Focus")), at(30)),
            Some(TimeEntry {
                side: break_side,
                start: at(20),
                end: at(30),
            })
        );
    }

    #[test]
    fn unknown_side_completes_the_active_entry_without_starting_another() {
        let work = side(1, "Work");
        let mut tracker = Tracker::default();

        tracker.on_side(Some(&work), at(10));

        assert_eq!(
            tracker.on_side(None, at(20)),
            Some(TimeEntry {
                side: work,
                start: at(10),
                end: at(20),
            })
        );
        assert_eq!(tracker.on_side(None, at(30)), None);
    }

    #[test]
    fn finishing_completes_and_clears_the_active_entry() {
        let work = side(1, "Work");
        let mut tracker = Tracker::default();
        tracker.on_side(Some(&work), at(10));

        assert_eq!(
            tracker.finish(at(20)),
            Some(TimeEntry {
                side: work,
                start: at(10),
                end: at(20),
            })
        );
        assert_eq!(tracker.finish(at(30)), None);
    }

    #[test]
    fn finishing_without_an_active_entry_returns_none() {
        assert_eq!(Tracker::default().finish(at(10)), None);
    }
}
