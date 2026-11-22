//! Lap and sector tracking plus standings. Pure logic.
//!
//! A lap only counts after passing both sector splits (1/3 and 2/3 of the lap),
//! so reversing over the line or cutting back does not score.

use std::cmp::Ordering;

/// Cars on the grid in a race: the player plus `FIELD_SIZE - 1` rivals.
pub const FIELD_SIZE: usize = 10;
/// Grid slot the player starts from (0 = pole), mid-pack.
pub const PLAYER_GRID_SLOT: usize = 5;
/// Seconds after GO during which contact bumps cars without spinning them.
pub const START_GRACE: f32 = 3.0;

/// Whether ordinary contact may spin a car at race time `clock`.
pub fn contact_spins(clock: f32) -> bool {
    clock >= START_GRACE
}

pub const SPLITS: [f32; 2] = [1.0 / 3.0, 2.0 / 3.0];
/// Largest believable progress change per update; larger jumps are ignored.
const MAX_STEP: f32 = 250.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LapEvent {
    /// Sector `index` (0 or 1) completed at `time` seconds into the lap.
    Sector { index: usize, time: f32 },
    /// A lap completed with this lap time.
    Lap { time: f32, sectors: [f32; 3] },
    /// The final lap completed; `time` is the total race time.
    Finish { time: f32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LapTracker {
    /// Accumulated forward distance since the start line (negative on the grid).
    pub total: f32,
    last: f32,
    pub laps: u32,
    passed: [bool; 2],
    pub lap_start: f32,
    /// Cumulative split times for the lap in progress.
    pub splits: [f32; 2],
    pub lap_times: Vec<f32>,
    pub best_lap: Option<f32>,
    pub finished_at: Option<f32>,
    pub total_laps: u32,
}

impl LapTracker {
    /// `progress` is the car's starting lap distance (usually just behind the line).
    pub fn new(track_len: f32, progress: f32, total_laps: u32) -> Self {
        let mut behind = progress.rem_euclid(track_len);
        if behind > track_len * 0.5 {
            behind -= track_len;
        }
        Self {
            total: behind,
            last: progress,
            laps: 0,
            passed: [false; 2],
            lap_start: 0.0,
            splits: [0.0; 2],
            lap_times: Vec::new(),
            best_lap: None,
            finished_at: None,
            total_laps,
        }
    }

    /// Current lap number for display, 1-based and capped at the total.
    pub fn display_lap(&self) -> u32 {
        (self.laps + 1).min(self.total_laps)
    }

    pub fn current_lap_time(&self, now: f32) -> f32 {
        (now - self.lap_start).max(0.0)
    }

    pub fn update(&mut self, track_len: f32, progress: f32, now: f32) -> Option<LapEvent> {
        if self.finished_at.is_some() {
            return None;
        }
        let mut d = (progress - self.last).rem_euclid(track_len);
        if d > track_len * 0.5 {
            d -= track_len;
        }
        if d.abs() > MAX_STEP {
            self.last = progress;
            return None;
        }
        let crossed_line = d > 0.0 && progress < self.last;
        self.last = progress;
        self.total += d;
        let frac = progress / track_len;

        if crossed_line && self.total >= 0.0 {
            if self.passed == [true, true] {
                let time = now - self.lap_start;
                let sectors = [
                    self.splits[0],
                    self.splits[1] - self.splits[0],
                    time - self.splits[1],
                ];
                self.laps += 1;
                self.lap_times.push(time);
                self.best_lap = Some(self.best_lap.map_or(time, |b| b.min(time)));
                self.lap_start = now;
                self.passed = [false; 2];
                if self.laps >= self.total_laps {
                    self.finished_at = Some(now);
                    return Some(LapEvent::Finish { time: now });
                }
                return Some(LapEvent::Lap { time, sectors });
            }
            // First crossing from the grid starts lap timing.
            if self.laps == 0 && self.lap_times.is_empty() && self.total < MAX_STEP {
                self.lap_start = now;
            }
        }

        for (k, &split) in SPLITS.iter().enumerate() {
            let in_range = frac >= split && frac < split + 0.25;
            let ready = k == 0 || self.passed[0];
            if !self.passed[k] && ready && in_range && self.total > 0.0 {
                self.passed[k] = true;
                let t = now - self.lap_start;
                self.splits[k] = t;
                return Some(LapEvent::Sector { index: k, time: t });
            }
        }
        None
    }
}

/// Order racers: finished cars by finish time, then the rest by distance.
pub fn standings(trackers: &[&LapTracker]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..trackers.len()).collect();
    idx.sort_by(|&a, &b| {
        let (ta, tb) = (trackers[a], trackers[b]);
        match (ta.finished_at, tb.finished_at) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => tb.total.partial_cmp(&ta.total).unwrap_or(Ordering::Equal),
        }
    });
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: f32 = 3000.0;

    /// Drive forward from `from` by `dist` in small steps, advancing time.
    fn drive(t: &mut LapTracker, pos: &mut f32, now: &mut f32, dist: f32) -> Vec<LapEvent> {
        let mut out = Vec::new();
        let steps = (dist.abs() / 10.0).ceil() as usize;
        for _ in 0..steps {
            *pos = (*pos + dist.signum() * 10.0).rem_euclid(L);
            *now += 0.02;
            if let Some(e) = t.update(L, *pos, *now) {
                out.push(e);
            }
        }
        out
    }

    #[test]
    fn contact_spins_only_after_the_start_grace() {
        assert!(!contact_spins(-1.0));
        assert!(!contact_spins(0.0));
        assert!(!contact_spins(START_GRACE - 0.01));
        assert!(contact_spins(START_GRACE));
    }

    #[test]
    fn three_laps_finish_with_sectors() {
        let mut pos = L - 60.0;
        let mut now = 0.0;
        let mut t = LapTracker::new(L, pos, 3);
        assert!(t.total < 0.0);
        let ev = drive(&mut t, &mut pos, &mut now, 60.0 + L * 3.0 + 5.0);
        let sectors = ev.iter().filter(|e| matches!(e, LapEvent::Sector { .. })).count();
        let laps = ev.iter().filter(|e| matches!(e, LapEvent::Lap { .. })).count();
        assert_eq!(sectors, 6);
        assert_eq!(laps, 2);
        assert!(matches!(ev.last(), Some(LapEvent::Finish { .. })));
        assert_eq!(t.lap_times.len(), 3);
        let lap = t.lap_times[0];
        assert!((lap - 6.0).abs() < 0.1, "lap {lap}");
    }

    #[test]
    fn reversing_over_the_line_does_not_count() {
        let mut pos = 100.0;
        let mut now = 0.0;
        let mut t = LapTracker::new(L, pos, 3);
        drive(&mut t, &mut pos, &mut now, -300.0);
        let ev = drive(&mut t, &mut pos, &mut now, 300.0);
        assert!(ev.iter().all(|e| !matches!(e, LapEvent::Lap { .. })));
        assert_eq!(t.laps, 0);
    }

    #[test]
    fn teleport_jumps_are_ignored() {
        let mut t = LapTracker::new(L, 10.0, 3);
        t.update(L, 1400.0, 1.0);
        assert!(t.total.abs() < 20.0);
    }

    #[test]
    fn lap_sector_times_sum_to_lap_time() {
        let mut pos = L - 20.0;
        let mut now = 0.0;
        let mut t = LapTracker::new(L, pos, 3);
        let ev = drive(&mut t, &mut pos, &mut now, L + 30.0);
        let lap = ev.iter().find_map(|e| match e {
            LapEvent::Lap { time, sectors } => Some((*time, *sectors)),
            _ => None,
        });
        let (time, s) = lap.expect("a lap");
        assert!((s.iter().sum::<f32>() - time).abs() < 1e-3);
    }

    #[test]
    fn standings_prefer_finishers_then_distance() {
        let mut a = LapTracker::new(L, 0.0, 3);
        let mut b = a.clone();
        let mut c = a.clone();
        a.total = 500.0;
        b.total = 900.0;
        c.finished_at = Some(80.0);
        assert_eq!(standings(&[&a, &b, &c]), vec![2, 1, 0]);
    }
}
