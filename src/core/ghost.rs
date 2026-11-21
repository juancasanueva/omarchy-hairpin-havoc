//! Time-trial ghosts: recording, playback interpolation, and compact
//! shareable text codes.

use std::f32::consts::TAU;
use std::fmt;

use bevy::math::Vec2;

use super::cars::CarId;
use super::tracks::TrackId;

/// Recording rate.
pub const SAMPLE_HZ: f32 = 30.0;
pub const SAMPLE_INTERVAL: f32 = 1.0 / SAMPLE_HZ;
/// Tolerance so frame-time jitter does not skip samples.
const SAMPLE_EPSILON: f32 = 1e-4;

pub const CODE_PREFIX: &str = "HH1:";
pub const CODE_VERSION: u8 = 1;
pub const MAX_LABEL_BYTES: usize = 24;
pub const MAX_LAP_TIME: f32 = 600.0;
pub const MIN_CODE_SAMPLES: usize = 2;
pub const MAX_CODE_SAMPLES: usize = 1800;
const SAMPLE_BYTES: usize = 8;
const SPEED_STEP: f32 = 3.0;
const FLAG_DRIFTING: u8 = 0b0000_0001;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GhostSample {
    /// Seconds since lap start.
    pub t: f32,
    pub pos: Vec2,
    /// Radians.
    pub heading: f32,
    pub speed: f32,
    pub drifting: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ghost {
    pub lap_time: f32,
    /// Cumulative split times: `sectors[i]` is the time since lap start at
    /// which sector `i` ended, so they strictly increase and
    /// `sectors[2] <= lap_time` (normally equal).
    pub sectors: [f32; 3],
    pub car: CarId,
    pub track: TrackId,
    pub label: String,
    /// Sorted by `t`.
    pub samples: Vec<GhostSample>,
}

impl Ghost {
    /// Interpolated state at lap time `t`, or `None` outside the recording.
    pub fn sample_at(&self, t: f32) -> Option<GhostSample> {
        let first = self.samples.first()?;
        let last = self.samples.last()?;
        if !(first.t..=last.t).contains(&t) {
            return None;
        }
        let idx = self.samples.partition_point(|s| s.t <= t);
        if idx == 0 {
            return None;
        }
        let a = self.samples[idx - 1];
        let Some(&b) = self.samples.get(idx) else {
            return Some(a);
        };
        let span = b.t - a.t;
        if span <= f32::EPSILON {
            return Some(a);
        }
        let f = (t - a.t) / span;
        Some(GhostSample {
            t,
            pos: a.pos.lerp(b.pos, f),
            heading: lerp_angle(a.heading, b.heading, f),
            speed: a.speed + (b.speed - a.speed) * f,
            drifting: if f < 0.5 { a.drifting } else { b.drifting },
        })
    }
}

/// Interpolates between two angles along the shortest arc.
fn lerp_angle(a: f32, b: f32, f: f32) -> f32 {
    let diff = (b - a + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI;
    a + diff * f
}

/// Collects lap samples at `SAMPLE_HZ`.
#[derive(Clone, Debug, Default)]
pub struct GhostRecorder {
    samples: Vec<GhostSample>,
}

impl GhostRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Stores a sample when at least one sample interval has passed since
    /// the last stored one; otherwise ignores it.
    pub fn record(&mut self, t: f32, pos: Vec2, heading: f32, speed: f32, drifting: bool) {
        if let Some(last) = self.samples.last()
            && t - last.t < SAMPLE_INTERVAL - SAMPLE_EPSILON
        {
            return;
        }
        self.samples.push(GhostSample {
            t,
            pos,
            heading,
            speed,
            drifting,
        });
    }

    pub fn finish(
        self,
        lap_time: f32,
        sectors: [f32; 3],
        car: CarId,
        track: TrackId,
        label: String,
    ) -> Ghost {
        Ghost {
            lap_time,
            sectors,
            car,
            track,
            label,
            samples: self.samples,
        }
    }
}

// ---------------------------------------------------------------------------
// Shareable codes
// ---------------------------------------------------------------------------
//
// A code is `HH1:` followed by unpadded base64url of this big-endian payload:
//
//   u8   version (1)
//   u8   track index (`TrackId::ALL`)
//   u8   car index (`CarId::ALL`)
//   u32  lap time, centiseconds
//   u32  x3 cumulative split times, centiseconds
//   u8   label byte length (<= 24), then that many UTF-8 bytes
//   u16  sample count (2..=1800)
//   8 bytes per sample:
//     u8   dt: centiseconds since the previous kept sample (first: since lap start)
//     i16  x, world px, rounded
//     i16  y, world px, rounded
//     u8   heading, 1/256 turn
//     u8   speed / 3, rounded, clamped to 255
//     u8   flags: bit 0 = drifting, bits 1..=7 reserved (must be 0)
//
// Every 2nd recorded sample is kept (a larger stride when the lap would exceed
// 1800 samples), and the final recorded sample is always kept. dt is computed
// from rounded cumulative times so rounding error never accumulates.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GhostCodeError {
    BadPrefix,
    BadBase64,
    UnsupportedVersion(u8),
    UnknownTrack(u8),
    UnknownCar(u8),
    BadLapTime,
    BadSplits,
    BadLabel,
    BadSampleCount(usize),
    BadFlags,
    BadLength,
}

impl fmt::Display for GhostCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GhostCodeError::BadPrefix => write!(f, "not a ghost code (missing {CODE_PREFIX})"),
            GhostCodeError::BadBase64 => write!(f, "ghost code contains invalid characters"),
            GhostCodeError::UnsupportedVersion(v) => write!(f, "unsupported ghost version {v}"),
            GhostCodeError::UnknownTrack(i) => write!(f, "unknown track {i}"),
            GhostCodeError::UnknownCar(i) => write!(f, "unknown car {i}"),
            GhostCodeError::BadLapTime => write!(f, "invalid lap time"),
            GhostCodeError::BadSplits => write!(f, "invalid sector splits"),
            GhostCodeError::BadLabel => write!(f, "invalid label"),
            GhostCodeError::BadSampleCount(n) => write!(f, "invalid sample count {n}"),
            GhostCodeError::BadFlags => write!(f, "invalid sample flags"),
            GhostCodeError::BadLength => write!(f, "ghost code has the wrong length"),
        }
    }
}

impl std::error::Error for GhostCodeError {}

fn to_centis(secs: f32) -> u32 {
    if secs.is_finite() && secs > 0.0 {
        (secs * 100.0).round().min(u32::MAX as f32) as u32
    } else {
        0
    }
}

fn truncate_label(label: &str) -> &str {
    if label.len() <= MAX_LABEL_BYTES {
        return label;
    }
    let mut end = MAX_LABEL_BYTES;
    while !label.is_char_boundary(end) {
        end -= 1;
    }
    &label[..end]
}

/// Indices of the recorded samples that go into a code.
fn kept_indices(n: usize) -> Vec<usize> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        // Duplicate a lone sample so the code still decodes.
        return vec![0, 0];
    }
    let stride = 2.max(n.div_ceil(MAX_CODE_SAMPLES - 1));
    let mut idx: Vec<usize> = (0..n).step_by(stride).collect();
    if idx.last() != Some(&(n - 1)) {
        idx.push(n - 1);
    }
    idx
}

/// Encodes a ghost as a shareable text code. Ghosts with no samples produce a
/// code that fails to decode.
pub fn encode_code(ghost: &Ghost) -> String {
    let label = truncate_label(&ghost.label);
    let kept = kept_indices(ghost.samples.len());
    let mut out = Vec::with_capacity(22 + label.len() + kept.len() * SAMPLE_BYTES);
    out.push(CODE_VERSION);
    out.push(ghost.track.index() as u8);
    out.push(ghost.car.index() as u8);
    out.extend_from_slice(&to_centis(ghost.lap_time).to_be_bytes());
    for split in ghost.sectors {
        out.extend_from_slice(&to_centis(split).to_be_bytes());
    }
    out.push(label.len() as u8);
    out.extend_from_slice(label.as_bytes());
    out.extend_from_slice(&(kept.len() as u16).to_be_bytes());

    let mut prev_cs: u32 = 0;
    for i in kept {
        let s = ghost.samples[i];
        let dt = to_centis(s.t).saturating_sub(prev_cs).min(255);
        prev_cs += dt;
        let x = s.pos.x.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        let y = s.pos.y.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        let turn = if s.heading.is_finite() {
            s.heading.rem_euclid(TAU) / TAU
        } else {
            0.0
        };
        let heading = ((turn * 256.0).round() as u32 % 256) as u8;
        let speed = if s.speed.is_finite() {
            (s.speed.max(0.0) / SPEED_STEP).round().min(255.0) as u8
        } else {
            0
        };
        let flags = if s.drifting { FLAG_DRIFTING } else { 0 };
        out.push(dt as u8);
        out.extend_from_slice(&x.to_be_bytes());
        out.extend_from_slice(&y.to_be_bytes());
        out.push(heading);
        out.push(speed);
        out.push(flags);
    }

    let mut code = String::from(CODE_PREFIX);
    code.push_str(&base64url_encode(&out));
    code
}

/// Byte cursor that reports `BadLength` when the payload runs out.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], GhostCodeError> {
        let end = self.pos.checked_add(n).ok_or(GhostCodeError::BadLength)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(GhostCodeError::BadLength)?;
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, GhostCodeError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, GhostCodeError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn i16(&mut self) -> Result<i16, GhostCodeError> {
        let b = self.take(2)?;
        Ok(i16::from_be_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, GhostCodeError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// Parses and validates a shareable ghost code. Surrounding whitespace is ignored.
pub fn decode_code(code: &str) -> Result<Ghost, GhostCodeError> {
    let body = code
        .trim()
        .strip_prefix(CODE_PREFIX)
        .ok_or(GhostCodeError::BadPrefix)?;
    let bytes = base64url_decode(body).ok_or(GhostCodeError::BadBase64)?;
    let mut r = Reader {
        bytes: &bytes,
        pos: 0,
    };

    let version = r.u8()?;
    if version != CODE_VERSION {
        return Err(GhostCodeError::UnsupportedVersion(version));
    }
    let track_i = r.u8()?;
    let track = *TrackId::ALL
        .get(usize::from(track_i))
        .ok_or(GhostCodeError::UnknownTrack(track_i))?;
    let car_i = r.u8()?;
    let car = CarId::from_index(usize::from(car_i)).ok_or(GhostCodeError::UnknownCar(car_i))?;

    let lap_cs = r.u32()?;
    if lap_cs == 0 || lap_cs > (MAX_LAP_TIME * 100.0) as u32 {
        return Err(GhostCodeError::BadLapTime);
    }
    let mut splits = [0u32; 3];
    for s in &mut splits {
        *s = r.u32()?;
    }
    let increasing = splits[0] > 0 && splits[0] < splits[1] && splits[1] < splits[2];
    if !increasing || splits[2] > lap_cs {
        return Err(GhostCodeError::BadSplits);
    }

    let label_len = usize::from(r.u8()?);
    if label_len > MAX_LABEL_BYTES {
        return Err(GhostCodeError::BadLabel);
    }
    let label = std::str::from_utf8(r.take(label_len)?)
        .map_err(|_| GhostCodeError::BadLabel)?
        .to_owned();

    let count = usize::from(r.u16()?);
    if !(MIN_CODE_SAMPLES..=MAX_CODE_SAMPLES).contains(&count) {
        return Err(GhostCodeError::BadSampleCount(count));
    }
    if bytes.len() != r.pos + count * SAMPLE_BYTES {
        return Err(GhostCodeError::BadLength);
    }

    let mut samples = Vec::with_capacity(count);
    let mut t_cs: u32 = 0;
    for _ in 0..count {
        t_cs += u32::from(r.u8()?);
        let x = r.i16()?;
        let y = r.i16()?;
        let heading = r.u8()?;
        let speed = r.u8()?;
        let flags = r.u8()?;
        if flags & !FLAG_DRIFTING != 0 {
            return Err(GhostCodeError::BadFlags);
        }
        samples.push(GhostSample {
            t: t_cs as f32 / 100.0,
            pos: Vec2::new(f32::from(x), f32::from(y)),
            heading: f32::from(heading) / 256.0 * TAU,
            speed: f32::from(speed) * SPEED_STEP,
            drifting: flags & FLAG_DRIFTING != 0,
        });
    }

    Ok(Ghost {
        lap_time: lap_cs as f32 / 100.0,
        sectors: splits.map(|s| s as f32 / 100.0),
        car,
        track,
        label,
        samples,
    })
}

// ---------------------------------------------------------------------------
// Base64url (RFC 4648 section 5, no padding)
// ---------------------------------------------------------------------------

const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let chars = chunk.len() + 1;
        for i in 0..chars {
            let sextet = (n >> (18 - 6 * i)) & 0x3f;
            out.push(B64_ALPHABET[sextet as usize] as char);
        }
    }
    out
}

fn b64_value(c: u8) -> Option<u32> {
    let v = match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'-' => 62,
        b'_' => 63,
        _ => return None,
    };
    Some(u32::from(v))
}

fn base64url_decode(text: &str) -> Option<Vec<u8>> {
    let input = text.as_bytes();
    if input.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3 + 2);
    for chunk in input.chunks(4) {
        let mut n: u32 = 0;
        for (i, &c) in chunk.iter().enumerate() {
            n |= b64_value(c)? << (18 - 6 * i);
        }
        let produced = chunk.len() - 1;
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        // Reject non-canonical trailing bits in a short final chunk.
        if bytes[produced..].iter().any(|&b| b != 0) && chunk.len() < 4 {
            return None;
        }
        out.extend_from_slice(&bytes[..produced]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn angle_diff(a: f32, b: f32) -> f32 {
        ((a - b + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI).abs()
    }

    /// A synthetic lap: a circle of radius 1500 px driven at 30 Hz.
    fn sample_ghost(secs: f32) -> Ghost {
        let mut rec = GhostRecorder::new();
        let mut t = 0.0;
        while t <= secs {
            let a = t / secs * TAU;
            let pos = Vec2::new(a.cos() * 1500.0, a.sin() * 1500.0 - 300.0);
            let heading = a + std::f32::consts::FRAC_PI_2 - std::f32::consts::PI;
            let speed = 200.0 + 150.0 * (a * 3.0).sin();
            rec.record(t, pos, heading, speed, (a * 5.0).sin() > 0.4);
            t += 1.0 / 120.0;
        }
        rec.finish(
            secs,
            [secs * 0.31, secs * 0.66, secs],
            CarId::Halcyon,
            TrackId::NeonDocks,
            "Ace driver".into(),
        )
    }

    #[test]
    fn recorder_samples_at_thirty_hz() {
        let mut rec = GhostRecorder::new();
        for i in 0..240 {
            rec.record(i as f32 / 240.0, Vec2::ZERO, 0.0, 0.0, false);
        }
        assert!((29..=31).contains(&rec.len()), "{}", rec.len());
        let g = rec.finish(
            1.0,
            [0.3, 0.6, 1.0],
            CarId::Pebble,
            TrackId::OrchardRing,
            String::new(),
        );
        for w in g.samples.windows(2) {
            assert!(w[1].t - w[0].t >= SAMPLE_INTERVAL - SAMPLE_EPSILON);
        }
    }

    #[test]
    fn sample_at_interpolates_and_bounds() {
        let ghost = Ghost {
            lap_time: 2.0,
            sectors: [0.5, 1.0, 2.0],
            car: CarId::Pebble,
            track: TrackId::OrchardRing,
            label: String::new(),
            samples: vec![
                GhostSample {
                    t: 1.0,
                    pos: Vec2::new(0.0, 0.0),
                    heading: 3.0,
                    speed: 100.0,
                    drifting: false,
                },
                GhostSample {
                    t: 2.0,
                    pos: Vec2::new(10.0, -20.0),
                    heading: -3.0,
                    speed: 200.0,
                    drifting: true,
                },
            ],
        };
        assert_eq!(ghost.sample_at(0.99), None);
        assert_eq!(ghost.sample_at(2.01), None);
        assert_eq!(ghost.sample_at(f32::NAN), None);
        let mid = ghost.sample_at(1.5).unwrap();
        assert!(mid.pos.distance(Vec2::new(5.0, -10.0)) < 1e-4);
        assert!((mid.speed - 150.0).abs() < 1e-4);
        assert!(mid.drifting);
        // Shortest arc from 3.0 to -3.0 rad crosses +/-PI, not zero.
        assert!(
            angle_diff(mid.heading, std::f32::consts::PI) < 1e-3,
            "{}",
            mid.heading
        );
        assert_eq!(ghost.sample_at(2.0).unwrap().pos, Vec2::new(10.0, -20.0));
        assert_eq!(ghost.sample_at(1.0).unwrap().pos, Vec2::ZERO);
    }

    #[test]
    fn code_round_trip() {
        let ghost = sample_ghost(42.37);
        let code = encode_code(&ghost);
        assert!(code.starts_with(CODE_PREFIX));
        let back = decode_code(&format!("  {code}\n")).unwrap();
        assert_eq!(back.car, ghost.car);
        assert_eq!(back.track, ghost.track);
        assert_eq!(back.label, ghost.label);
        assert!((back.lap_time - ghost.lap_time).abs() <= 0.01);
        for i in 0..3 {
            assert!((back.sectors[i] - ghost.sectors[i]).abs() <= 0.01);
        }
        assert!(back.samples.len() >= ghost.samples.len() / 2);
        assert_eq!(
            back.samples.last().unwrap().t,
            (ghost.samples.last().unwrap().t * 100.0).round() / 100.0
        );
        // Each decoded sample matches the original at the same time.
        for s in &back.samples {
            let orig = ghost
                .samples
                .iter()
                .min_by(|a, b| (a.t - s.t).abs().total_cmp(&(b.t - s.t).abs()))
                .unwrap();
            assert!((orig.t - s.t).abs() <= 0.01, "t {} vs {}", orig.t, s.t);
            assert!((orig.pos.x - s.pos.x).abs() <= 1.0);
            assert!((orig.pos.y - s.pos.y).abs() <= 1.0);
            assert!(angle_diff(orig.heading, s.heading) <= TAU / 256.0);
            assert!((orig.speed - s.speed).abs() <= SPEED_STEP);
            assert_eq!(orig.drifting, s.drifting);
        }
    }

    #[test]
    fn long_laps_use_a_larger_stride() {
        let ghost = sample_ghost(150.0);
        assert!(ghost.samples.len() > 2 * MAX_CODE_SAMPLES);
        let back = decode_code(&encode_code(&ghost)).unwrap();
        assert!(back.samples.len() <= MAX_CODE_SAMPLES);
        let last = back.samples.last().unwrap().t;
        assert!((last - ghost.samples.last().unwrap().t).abs() <= 0.01);
    }

    #[test]
    fn label_is_truncated_on_char_boundary() {
        let mut ghost = sample_ghost(10.0);
        ghost.label = "ééééééééééééééé".into(); // 30 bytes
        let back = decode_code(&encode_code(&ghost)).unwrap();
        assert_eq!(back.label, "éééééééééééé");
        assert!(back.label.len() <= MAX_LABEL_BYTES);
    }

    fn payload(ghost: &Ghost) -> Vec<u8> {
        base64url_decode(&encode_code(ghost)[CODE_PREFIX.len()..]).unwrap()
    }

    fn code_from(bytes: &[u8]) -> String {
        format!("{CODE_PREFIX}{}", base64url_encode(bytes))
    }

    #[test]
    fn decode_rejects_bad_input() {
        let ghost = sample_ghost(10.0);
        let good = payload(&ghost);
        let label_len = ghost.label.len();
        let count_at = 20 + label_len;

        assert_eq!(decode_code("HH2:AAAA"), Err(GhostCodeError::BadPrefix));
        assert_eq!(decode_code("HH1:@@@@"), Err(GhostCodeError::BadBase64));
        assert_eq!(decode_code("HH1:A"), Err(GhostCodeError::BadBase64));

        let mut b = good.clone();
        b[0] = 2;
        assert_eq!(
            decode_code(&code_from(&b)),
            Err(GhostCodeError::UnsupportedVersion(2))
        );
        let mut b = good.clone();
        b[1] = 4;
        assert_eq!(
            decode_code(&code_from(&b)),
            Err(GhostCodeError::UnknownTrack(4))
        );
        let mut b = good.clone();
        b[2] = 7;
        assert_eq!(
            decode_code(&code_from(&b)),
            Err(GhostCodeError::UnknownCar(7))
        );
        let mut b = good.clone();
        b[3..7].copy_from_slice(&60_001u32.to_be_bytes());
        assert_eq!(decode_code(&code_from(&b)), Err(GhostCodeError::BadLapTime));
        let mut b = good.clone();
        b.copy_within(11..15, 7);
        assert_eq!(decode_code(&code_from(&b)), Err(GhostCodeError::BadSplits));
        let mut b = good.clone();
        b[19] = 25;
        assert_eq!(decode_code(&code_from(&b)), Err(GhostCodeError::BadLabel));
        let mut b = good.clone();
        b[count_at..count_at + 2].copy_from_slice(&1u16.to_be_bytes());
        assert_eq!(
            decode_code(&code_from(&b)),
            Err(GhostCodeError::BadSampleCount(1))
        );
        let mut b = good.clone();
        b[count_at..count_at + 2].copy_from_slice(&1801u16.to_be_bytes());
        assert_eq!(
            decode_code(&code_from(&b)),
            Err(GhostCodeError::BadSampleCount(1801))
        );
        let mut b = good.clone();
        b.push(0);
        assert_eq!(decode_code(&code_from(&b)), Err(GhostCodeError::BadLength));
        let b = &good[..good.len() - 1];
        assert_eq!(decode_code(&code_from(b)), Err(GhostCodeError::BadLength));
        let mut b = good.clone();
        let last = b.len() - 1;
        b[last] |= 0b10;
        assert_eq!(decode_code(&code_from(&b)), Err(GhostCodeError::BadFlags));
        assert_eq!(
            decode_code(&code_from(&good[..10])),
            Err(GhostCodeError::BadLength)
        );
    }

    #[test]
    fn base64url_round_trip() {
        for len in 0..20 {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 250) as u8).collect();
            let text = base64url_encode(&data);
            assert!(!text.contains('='));
            assert_eq!(base64url_decode(&text).unwrap(), data);
        }
        assert_eq!(base64url_encode(b"\xfb\xff"), "-_8");
    }
}
