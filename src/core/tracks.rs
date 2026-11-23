//! The ten circuits. Control points are listed in driving order;
//! the start/finish line sits at control point 0.

use super::track::{BranchDef, BranchKind, ControlPoint, Track, cp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackId {
    OrchardRing,
    DuneSprint,
    NeonDocks,
    GlacierPass,
    EmberOval,
    MapleLoop,
    SaltFlats,
    IronHarbor,
    AuroraRidge,
    JungleTwist,
}

impl TrackId {
    /// Saves and ghost codes store positions in this list: only ever append.
    pub const ALL: [TrackId; 10] = [
        TrackId::OrchardRing,
        TrackId::DuneSprint,
        TrackId::NeonDocks,
        TrackId::GlacierPass,
        TrackId::EmberOval,
        TrackId::MapleLoop,
        TrackId::SaltFlats,
        TrackId::IronHarbor,
        TrackId::AuroraRidge,
        TrackId::JungleTwist,
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
            TrackId::EmberOval => "ember-oval",
            TrackId::MapleLoop => "maple-loop",
            TrackId::SaltFlats => "salt-flats",
            TrackId::IronHarbor => "iron-harbor",
            TrackId::AuroraRidge => "aurora-ridge",
            TrackId::JungleTwist => "jungle-twist",
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
        // Two straights at y = -400 and 400 joined by half circles of radius
        // 400 around (1700, 0) and (-1700, 0), one point every 22.5 degrees. The
        // points 80 px before and after each turn keep the straights straight.
        TrackId::EmberOval => TrackDef {
            id,
            name: "Ember Oval",
            difficulty: Difficulty::Fast,
            main: vec![
                cp(-700.0, -400.0, 80.0),
                cp(500.0, -400.0, 80.0),
                cp(1620.0, -400.0, 80.0),
                cp(1700.0, -400.0, 80.0),
                cp(1853.1, -369.6, 80.0),
                cp(1982.8, -282.8, 80.0),
                cp(2069.6, -153.1, 80.0),
                cp(2100.0, 0.0, 80.0),
                cp(2069.6, 153.1, 80.0),
                cp(1982.8, 282.8, 80.0),
                cp(1853.1, 369.6, 80.0),
                cp(1700.0, 400.0, 80.0),
                cp(1620.0, 400.0, 80.0),
                cp(0.0, 400.0, 80.0),
                cp(-1620.0, 400.0, 80.0),
                cp(-1700.0, 400.0, 80.0),
                cp(-1853.1, 369.6, 80.0),
                cp(-1982.8, 282.8, 80.0),
                cp(-2069.6, 153.1, 80.0),
                cp(-2100.0, 0.0, 80.0),
                cp(-2069.6, -153.1, 80.0),
                cp(-1982.8, -282.8, 80.0),
                cp(-1853.1, -369.6, 80.0),
                cp(-1700.0, -400.0, 80.0),
                cp(-1620.0, -400.0, 80.0),
            ],
            branches: vec![
                // Infield cut inside the right-hand turn.
                BranchDef {
                    kind: BranchKind::Shortcut,
                    from: 4.0,
                    to: 10.0,
                    mids: vec![cp(1905.0, -170.0, 34.0), cp(1905.0, 170.0, 34.0)],
                },
                // Wide lane around the left-hand turn.
                BranchDef {
                    kind: BranchKind::BoostLane,
                    from: 15.0,
                    to: 23.0,
                    mids: vec![
                        cp(-2160.0, 500.0, 42.0),
                        cp(-2400.0, 0.0, 42.0),
                        cp(-2160.0, -500.0, 42.0),
                    ],
                },
            ],
            box_stations: [0.06, 0.42, 0.70],
        },
        TrackId::MapleLoop => TrackDef {
            id,
            name: "Maple Loop",
            difficulty: Difficulty::Balanced,
            main: vec![
                cp(-1400.0, -950.0, 62.0),
                cp(0.0, -1000.0, 64.0),
                cp(1300.0, -950.0, 62.0),
                cp(1900.0, -600.0, 58.0),
                cp(2000.0, 0.0, 56.0),
                cp(1700.0, 500.0, 56.0),
                cp(1100.0, 600.0, 54.0),
                cp(700.0, 300.0, 54.0),
                cp(200.0, 350.0, 54.0),
                cp(-100.0, 750.0, 56.0),
                cp(300.0, 1150.0, 58.0),
                cp(-300.0, 1450.0, 60.0),
                cp(-1100.0, 1400.0, 60.0),
                cp(-1700.0, 1000.0, 58.0),
                cp(-1500.0, 450.0, 54.0),
                cp(-1900.0, -100.0, 56.0),
                cp(-1850.0, -650.0, 60.0),
            ],
            branches: vec![
                BranchDef {
                    kind: BranchKind::Shortcut,
                    from: 6.0,
                    to: 9.0,
                    mids: vec![cp(500.0, 700.0, 34.0)],
                },
                BranchDef {
                    kind: BranchKind::BoostLane,
                    from: 2.5,
                    to: 5.4,
                    mids: vec![
                        cp(2250.0, -500.0, 40.0),
                        cp(2350.0, 150.0, 40.0),
                        cp(2150.0, 650.0, 40.0),
                    ],
                },
            ],
            box_stations: [0.05, 0.38, 0.70],
        },
        TrackId::SaltFlats => TrackDef {
            id,
            name: "Salt Flats",
            difficulty: Difficulty::Fast,
            main: vec![
                cp(-2100.0, -900.0, 68.0),
                cp(-400.0, -1000.0, 70.0),
                cp(1300.0, -950.0, 70.0),
                cp(2200.0, -600.0, 66.0),
                cp(2400.0, 100.0, 64.0),
                cp(2000.0, 800.0, 64.0),
                cp(1200.0, 1200.0, 66.0),
                cp(300.0, 1250.0, 66.0),
                cp(-500.0, 800.0, 64.0),
                cp(-1300.0, 1200.0, 66.0),
                cp(-2100.0, 900.0, 66.0),
                cp(-2500.0, 200.0, 66.0),
                cp(-2500.0, -500.0, 68.0),
            ],
            branches: vec![
                BranchDef {
                    kind: BranchKind::Shortcut,
                    from: 7.0,
                    to: 9.0,
                    mids: vec![cp(-500.0, 1330.0, 36.0)],
                },
                BranchDef {
                    kind: BranchKind::BoostLane,
                    from: 3.0,
                    to: 5.0,
                    mids: vec![cp(2700.0, -400.0, 42.0), cp(2750.0, 400.0, 42.0)],
                },
            ],
            box_stations: [0.06, 0.40, 0.72],
        },
        TrackId::IronHarbor => TrackDef {
            id,
            name: "Iron Harbor",
            difficulty: Difficulty::Technical,
            main: vec![
                cp(-1500.0, -900.0, 56.0),
                cp(300.0, -900.0, 58.0),
                cp(900.0, -700.0, 52.0),
                cp(1000.0, -200.0, 52.0),
                cp(600.0, 100.0, 52.0),
                cp(700.0, 500.0, 52.0),
                cp(1300.0, 600.0, 52.0),
                cp(1700.0, 1000.0, 52.0),
                cp(1400.0, 1400.0, 54.0),
                cp(700.0, 1350.0, 54.0),
                cp(300.0, 900.0, 52.0),
                cp(-300.0, 1300.0, 52.0),
                cp(-1000.0, 1300.0, 54.0),
                cp(-1200.0, 800.0, 52.0),
                cp(-700.0, 500.0, 52.0),
                cp(-900.0, 0.0, 52.0),
                cp(-1600.0, -100.0, 54.0),
                cp(-1900.0, -500.0, 56.0),
            ],
            branches: vec![
                BranchDef {
                    kind: BranchKind::Shortcut,
                    from: 6.7,
                    to: 8.6,
                    mids: vec![cp(1350.0, 1100.0, 32.0)],
                },
                BranchDef {
                    kind: BranchKind::BoostLane,
                    from: 11.0,
                    to: 13.0,
                    mids: vec![
                        cp(-650.0, 1600.0, 40.0),
                        cp(-1400.0, 1500.0, 40.0),
                        cp(-1550.0, 1050.0, 40.0),
                    ],
                },
            ],
            box_stations: [0.05, 0.36, 0.68],
        },
        TrackId::AuroraRidge => TrackDef {
            id,
            name: "Aurora Ridge",
            difficulty: Difficulty::Balanced,
            main: vec![
                cp(-1500.0, -1000.0, 62.0),
                cp(100.0, -1000.0, 64.0),
                cp(1200.0, -900.0, 60.0),
                cp(1800.0, -400.0, 58.0),
                cp(1350.0, 100.0, 56.0),
                cp(1900.0, 600.0, 56.0),
                cp(1600.0, 1200.0, 58.0),
                cp(800.0, 1300.0, 58.0),
                cp(300.0, 900.0, 56.0),
                cp(-300.0, 1300.0, 56.0),
                cp(-1100.0, 1350.0, 58.0),
                cp(-1800.0, 900.0, 58.0),
                cp(-1900.0, 200.0, 56.0),
                cp(-1300.0, -200.0, 56.0),
                cp(-2000.0, -600.0, 58.0),
            ],
            branches: vec![
                BranchDef {
                    kind: BranchKind::Shortcut,
                    from: 7.0,
                    to: 9.0,
                    mids: vec![cp(250.0, 1380.0, 34.0)],
                },
                BranchDef {
                    kind: BranchKind::BoostLane,
                    from: 2.6,
                    to: 5.6,
                    mids: vec![
                        cp(2200.0, -650.0, 40.0),
                        cp(2500.0, -50.0, 40.0),
                        cp(2450.0, 600.0, 40.0),
                        cp(2050.0, 1000.0, 40.0),
                    ],
                },
            ],
            box_stations: [0.05, 0.37, 0.69],
        },
        TrackId::JungleTwist => TrackDef {
            id,
            name: "Jungle Twist",
            difficulty: Difficulty::Technical,
            main: vec![
                cp(-1600.0, -900.0, 56.0),
                cp(0.0, -900.0, 58.0),
                cp(800.0, -1000.0, 54.0),
                cp(1400.0, -700.0, 52.0),
                cp(1200.0, -200.0, 52.0),
                cp(1700.0, 200.0, 52.0),
                cp(1500.0, 700.0, 52.0),
                cp(900.0, 600.0, 52.0),
                cp(600.0, 1000.0, 52.0),
                cp(1000.0, 1400.0, 54.0),
                cp(300.0, 1600.0, 54.0),
                cp(-400.0, 1300.0, 52.0),
                cp(-300.0, 800.0, 52.0),
                cp(-800.0, 500.0, 52.0),
                cp(-1400.0, 900.0, 54.0),
                cp(-1900.0, 500.0, 54.0),
                cp(-1500.0, 0.0, 52.0),
                cp(-2000.0, -500.0, 54.0),
            ],
            branches: vec![
                BranchDef {
                    kind: BranchKind::Shortcut,
                    from: 12.0,
                    to: 14.0,
                    mids: vec![cp(-850.0, 880.0, 32.0)],
                },
                BranchDef {
                    kind: BranchKind::BoostLane,
                    from: 3.0,
                    to: 5.5,
                    mids: vec![cp(1950.0, -600.0, 40.0), cp(2250.0, 0.0, 40.0)],
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

    /// Saves and ghost codes store list positions, so the order is a file format.
    #[test]
    fn track_order_is_stable() {
        let keys = TrackId::ALL.map(TrackId::key);
        assert_eq!(
            keys,
            [
                "orchard-ring",
                "dune-sprint",
                "neon-docks",
                "glacier-pass",
                "ember-oval",
                "maple-loop",
                "salt-flats",
                "iron-harbor",
                "aurora-ridge",
                "jungle-twist",
            ]
        );
    }

    /// Surface queries find the road from two 96 px grid cells away.
    #[test]
    fn walls_stay_within_the_lookup_reach() {
        for id in TrackId::ALL {
            let t = track_def(id).build();
            for path in t.paths() {
                for i in 0..path.samples.len() {
                    let reach = path.wall_offset(i, true).max(path.wall_offset(i, false));
                    assert!(reach < 180.0, "{id:?}: wall {reach:.0}px from the centerline");
                }
            }
        }
    }

    #[test]
    fn ember_oval_is_an_oval() {
        let t = track_def(TrackId::EmberOval).build();
        let mut turned = 0.0;
        for s in &t.main.samples {
            turned += s.curvature * crate::core::track::SAMPLE_SPACING;
            if s.pos.x.abs() < 1500.0 {
                assert!(s.curvature.abs() < 1e-4, "bent straight at {}", s.pos);
                assert!((s.pos.y.abs() - 400.0).abs() < 1.0, "straight off line at {}", s.pos);
            } else if s.pos.x.abs() > 1800.0 {
                let radius = s.pos.distance(bevy::math::Vec2::new(1700.0 * s.pos.x.signum(), 0.0));
                assert!((radius - 400.0).abs() < 4.0, "uneven turn at {}: {radius:.0}", s.pos);
                assert!(s.kerb, "no kerb at {}", s.pos);
            }
        }
        // One full left-hand revolution.
        assert!((turned - std::f32::consts::TAU).abs() < 0.05, "turned {turned}");
    }

    #[test]
    fn ember_oval_routes_trade_distance() {
        let t = track_def(TrackId::EmberOval).build();
        for b in &t.branches {
            let ratio = b.path.length / t.delta(b.start, b.end);
            match b.kind {
                BranchKind::Shortcut => assert!((0.75..0.9).contains(&ratio), "shortcut {ratio:.2}"),
                BranchKind::BoostLane => assert!((1.3..1.8).contains(&ratio), "boost lane {ratio:.2}"),
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
