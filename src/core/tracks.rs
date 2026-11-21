//! The four original circuits. Control points are listed in driving order;
//! the start/finish line sits at control point 0.

use super::track::{BranchDef, BranchKind, ControlPoint, Track, cp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackId {
    OrchardRing,
    DuneSprint,
    NeonDocks,
    GlacierPass,
}

impl TrackId {
    pub const ALL: [TrackId; 4] = [
        TrackId::OrchardRing,
        TrackId::DuneSprint,
        TrackId::NeonDocks,
        TrackId::GlacierPass,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&t| t == self).unwrap_or(0)
    }

    pub fn key(self) -> &'static str {
        match self {
            TrackId::OrchardRing => "orchard-ring",
            TrackId::DuneSprint => "dune-sprint",
            TrackId::NeonDocks => "neon-docks",
            TrackId::GlacierPass => "glacier-pass",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Difficulty {
    Balanced,
    Fast,
    Technical,
}

impl Difficulty {
    pub fn label(self) -> &'static str {
        match self {
            Difficulty::Balanced => "BALANCED",
            Difficulty::Fast => "FAST",
            Difficulty::Technical => "TECHNICAL",
        }
    }
}

pub struct TrackDef {
    pub id: TrackId,
    pub name: &'static str,
    pub difficulty: Difficulty,
    pub main: Vec<ControlPoint>,
    pub branches: Vec<BranchDef>,
    /// Lap fractions of the lucky-box stations.
    pub box_stations: [f32; 3],
}

impl TrackDef {
    pub fn build(&self) -> Track {
        Track::build(&self.main, &self.branches)
    }
}

pub const LAPS: u32 = 3;

pub fn track_def(id: TrackId) -> TrackDef {
    match id {
        TrackId::OrchardRing => TrackDef {
            id,
            name: "Orchard Ring",
            difficulty: Difficulty::Balanced,
            main: vec![
                cp(-1200.0, -900.0, 62.0),
                cp(0.0, -950.0, 64.0),
                cp(1200.0, -900.0, 62.0),
                cp(1700.0, -600.0, 58.0),
                cp(1800.0, -100.0, 56.0),
                cp(1500.0, 250.0, 54.0),
                cp(1100.0, 200.0, 54.0),
                cp(850.0, 520.0, 54.0),
                cp(1200.0, 850.0, 56.0),
                cp(1600.0, 1000.0, 56.0),
                cp(1400.0, 1350.0, 58.0),
                cp(600.0, 1350.0, 60.0),
                cp(0.0, 880.0, 56.0),
                cp(-500.0, 1250.0, 58.0),
                cp(-1200.0, 1200.0, 60.0),
                cp(-1700.0, 800.0, 58.0),
                cp(-1450.0, 300.0, 54.0),
                cp(-1800.0, -200.0, 56.0),
                cp(-1700.0, -700.0, 60.0),
            ],
            branches: vec![
                BranchDef {
                    kind: BranchKind::Shortcut,
                    from: 11.35,
                    to: 12.75,
                    mids: vec![cp(200.0, 1290.0, 34.0), cp(-200.0, 1260.0, 34.0)],
                },
                BranchDef {
                    kind: BranchKind::BoostLane,
                    from: 3.0,
                    to: 5.0,
                    mids: vec![cp(2080.0, -450.0, 40.0), cp(2120.0, 0.0, 40.0)],
                },
            ],
            box_stations: [0.05, 0.38, 0.70],
        },
        TrackId::DuneSprint => TrackDef {
            id,
            name: "Dune Sprint",
            difficulty: Difficulty::Fast,
            main: vec![
                cp(-2000.0, -600.0, 66.0),
                cp(-500.0, -800.0, 68.0),
                cp(1000.0, -800.0, 68.0),
                cp(2000.0, -500.0, 64.0),
                cp(2300.0, 200.0, 62.0),
                cp(1800.0, 700.0, 62.0),
                cp(800.0, 600.0, 64.0),
                cp(200.0, 900.0, 62.0),
                cp(-200.0, 1400.0, 62.0),
                cp(-1000.0, 1500.0, 64.0),
                cp(-1900.0, 1100.0, 64.0),
                cp(-2250.0, 350.0, 66.0),
            ],
            branches: vec![
                BranchDef {
                    kind: BranchKind::Shortcut,
                    from: 7.4,
                    to: 8.8,
                    mids: vec![cp(-300.0, 1120.0, 36.0)],
                },
                BranchDef {
                    kind: BranchKind::BoostLane,
                    from: 2.0,
                    to: 3.7,
                    mids: vec![cp(1600.0, -1100.0, 42.0), cp(2380.0, -820.0, 42.0)],
                },
            ],
            box_stations: [0.06, 0.40, 0.72],
        },
        TrackId::NeonDocks => TrackDef {
            id,
            name: "Neon Docks",
            difficulty: Difficulty::Technical,
            main: vec![
                cp(-1400.0, -800.0, 58.0),
                cp(200.0, -800.0, 60.0),
                cp(520.0, -500.0, 52.0),
                cp(500.0, 0.0, 52.0),
                cp(900.0, 300.0, 52.0),
                cp(1400.0, 300.0, 54.0),
                cp(1650.0, 700.0, 52.0),
                cp(1300.0, 1100.0, 52.0),
                cp(500.0, 1100.0, 54.0),
                cp(200.0, 700.0, 52.0),
                cp(-300.0, 700.0, 52.0),
                cp(-600.0, 1100.0, 52.0),
                cp(-1200.0, 1100.0, 54.0),
                cp(-1550.0, 600.0, 52.0),
                cp(-1100.0, 200.0, 52.0),
                cp(-1650.0, -250.0, 54.0),
            ],
            branches: vec![
                BranchDef {
                    kind: BranchKind::Shortcut,
                    from: 6.5,
                    to: 8.5,
                    mids: vec![cp(1000.0, 820.0, 32.0)],
                },
                BranchDef {
                    kind: BranchKind::BoostLane,
                    from: 0.2,
                    to: 1.0,
                    mids: vec![cp(-700.0, -1120.0, 40.0), cp(-100.0, -1100.0, 40.0)],
                },
            ],
            box_stations: [0.05, 0.36, 0.68],
        },
        TrackId::GlacierPass => TrackDef {
            id,
            name: "Glacier Pass",
            difficulty: Difficulty::Technical,
            main: vec![
                cp(-1600.0, -1000.0, 58.0),
                cp(0.0, -1000.0, 60.0),
                cp(900.0, -950.0, 56.0),
                cp(1150.0, -550.0, 52.0),
                cp(650.0, -300.0, 52.0),
                cp(500.0, 100.0, 52.0),
                cp(1200.0, 300.0, 54.0),
                cp(1500.0, 800.0, 54.0),
                cp(1000.0, 1200.0, 54.0),
                cp(200.0, 1000.0, 54.0),
                cp(-200.0, 1300.0, 52.0),
                cp(-900.0, 1300.0, 54.0),
                cp(-1050.0, 800.0, 52.0),
                cp(-400.0, 500.0, 52.0),
                cp(-600.0, 0.0, 52.0),
                cp(-1400.0, 100.0, 54.0),
                cp(-1900.0, -400.0, 56.0),
            ],
            branches: vec![
                BranchDef {
                    kind: BranchKind::Shortcut,
                    from: 9.4,
                    to: 10.8,
                    mids: vec![cp(-300.0, 1060.0, 32.0)],
                },
                BranchDef {
                    kind: BranchKind::BoostLane,
                    from: 6.0,
                    to: 8.0,
                    mids: vec![cp(1820.0, 480.0, 40.0), cp(1780.0, 1150.0, 40.0)],
                },
            ],
            box_stations: [0.05, 0.37, 0.69],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::track::Surface;

    #[test]
    fn every_track_builds_with_two_branches() {
        for id in TrackId::ALL {
            let t = track_def(id).build();
            assert!(t.length() > 5000.0, "{id:?} too short: {}", t.length());
            assert_eq!(t.branches.len(), 2, "{id:?}");
            assert_eq!(t.pads().count(), 2, "{id:?}");
        }
    }

    #[test]
    fn branches_leave_the_main_road() {
        for id in TrackId::ALL {
            let t = track_def(id).build();
            for (bi, b) in t.branches.iter().enumerate() {
                let (mid, _) = b.path.sample_at(b.path.length * 0.5);
                let q = t.query(mid);
                assert_eq!(q.route, Some(bi), "{id:?} branch {bi} mid snaps to main");
                assert!(
                    t.delta(b.start, b.end) > 0.0,
                    "{id:?} branch {bi} runs backwards"
                );
            }
        }
    }

    /// Distant parts of the main loop must not share drivable ground.
    #[test]
    fn main_loop_never_overlaps_itself() {
        for id in TrackId::ALL {
            let t = track_def(id).build();
            let s = &t.main.samples;
            let step = 4;
            for i in (0..s.len()).step_by(step) {
                for j in (0..s.len()).step_by(step) {
                    let along = t.delta(s[i].distance, s[j].distance).abs();
                    if along < 600.0 {
                        continue;
                    }
                    let min_gap = s[i].half_width + s[j].half_width + 60.0;
                    let gap = s[i].pos.distance(s[j].pos);
                    assert!(
                        gap > min_gap,
                        "{id:?}: samples {i} and {j} only {gap:.0}px apart"
                    );
                }
            }
        }
    }

    #[test]
    fn start_line_is_road() {
        for id in TrackId::ALL {
            let t = track_def(id).build();
            assert_eq!(t.query(t.main.samples[0].pos).surface, Surface::Road);
        }
    }
}
