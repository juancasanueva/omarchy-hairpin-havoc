//! Procedural audio: a live-synthesized player engine and pre-rendered sound effects.
//!
//! No audio files are used. The engine is a custom [`Decodable`] whose decoder reads
//! live parameters from shared atomics, and every sound effect is synthesized once at
//! startup into a sample buffer played through its own custom [`Decodable`].
//!
//! [`GameAudioPlugin`] must be added after Bevy's `AudioPlugin` (part of `DefaultPlugins`).

use std::f32::consts::{PI, TAU};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

use bevy::audio::{AddAudioSource, ChannelCount, SampleRate, Source, Volume};
use bevy::prelude::*;
use bevy::reflect::TypePath;

/// Output sample rate for every synthesized source.
pub const SAMPLE_RATE: u32 = 44_100;
const SR: f32 = SAMPLE_RATE as f32;

// ---------------------------------------------------------------------------
// Public interface
// ---------------------------------------------------------------------------

/// Registers the procedural audio sources, resources, the [`PlaySfx`] message and systems.
pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<EngineSynth>()
            .add_audio_source::<SfxClip>()
            .add_message::<PlaySfx>()
            .init_resource::<AudioSettings>()
            .init_resource::<EngineAudio>()
            .insert_resource(EngineLink(Arc::new(EngineShared::default())))
            .add_systems(Startup, (spawn_engine, build_sfx_bank))
            .add_systems(PostUpdate, (sync_engine, play_sfx));
    }
}

/// Global audio settings. Muted silences the engine and skips effects; volume scales everything.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct AudioSettings {
    pub muted: bool,
    pub volume: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            muted: false,
            volume: 0.7,
        }
    }
}

/// Player engine state, written by the race systems every frame.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct EngineAudio {
    /// False in menus: the engine fades out quickly.
    pub active: bool,
    /// Car index 0..=6.
    pub profile: usize,
    /// 0..1 fraction between idle and rev limit.
    pub rpm: f32,
    /// 0..1.
    pub throttle: f32,
    /// 0..1 nitro/boost intensity (adds turbo whistle and airflow hiss).
    pub boost: f32,
}

/// Request to play one sound effect.
#[derive(Message, Clone, Copy, Debug)]
pub struct PlaySfx(pub Sfx);

/// Every sound effect the game can request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sfx {
    CountdownTick,
    CountdownGo,
    SectorChime,
    LapComplete,
    FinishSlam,
    GearShift,
    /// Tier 1..=3; higher tiers are higher pitched and longer.
    BoostPop {
        tier: u8,
    },
    NitroIgnite,
    PadBoost,
    /// Strength 0..1 scales the volume.
    Bump {
        strength: f32,
    },
    Spinout,
    ItemPickup,
    Thumper,
    Slick,
    StaticBurst,
    BatteringBoost,
    AllTerrain,
    Phantom,
    DriftSurge,
    Jammer,
    MenuMove,
    MenuSelect,
    Coin,
    Purchase,
    Denied,
}

// ---------------------------------------------------------------------------
// Engine profiles
// ---------------------------------------------------------------------------

/// Synthesis parameters of one car's engine. Frequencies are firing-pulse rates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EngineProfile {
    pub name: &'static str,
    pub cylinders: u8,
    pub idle_hz: f32,
    pub limit_hz: f32,
    pub pipe_hz: f32,
    pub intake_hz: f32,
    /// 0..1 firing unevenness (amplitude and timing per cylinder).
    pub irregularity: f32,
    /// 0..1 turbo whistle amount.
    pub turbo: f32,
    /// Seed of the per-cylinder irregularity pattern.
    pub seed: u32,
}

/// Engine profiles in car index order.
pub const ENGINE_PROFILES: [EngineProfile; 7] = [
    // Small three-cylinder hatch: ~800..6500 rpm, uneven triple thrum.
    EngineProfile {
        name: "Pebble",
        cylinders: 3,
        idle_hz: 20.0,
        limit_hz: 162.0,
        pipe_hz: 180.0,
        intake_hz: 520.0,
        irregularity: 0.35,
        turbo: 0.0,
        seed: 0x0B1E_0001,
    },
    // Light four-cylinder coupe: ~900..7500 rpm, clean and buzzy.
    EngineProfile {
        name: "Sparrow",
        cylinders: 4,
        idle_hz: 30.0,
        limit_hz: 250.0,
        pipe_hz: 240.0,
        intake_hz: 700.0,
        irregularity: 0.12,
        turbo: 0.0,
        seed: 0x0B1E_0002,
    },
    // Rally turbo four with unequal headers: burbly, strong whistle.
    EngineProfile {
        name: "Bandit",
        cylinders: 4,
        idle_hz: 28.0,
        limit_hz: 240.0,
        pipe_hz: 150.0,
        intake_hz: 450.0,
        irregularity: 0.45,
        turbo: 1.0,
        seed: 0x0B1E_0003,
    },
    // High-revving six: ~1000..9000 rpm, smooth and bright.
    EngineProfile {
        name: "Mako",
        cylinders: 6,
        idle_hz: 50.0,
        limit_hz: 450.0,
        pipe_hz: 320.0,
        intake_hz: 900.0,
        irregularity: 0.08,
        turbo: 0.3,
        seed: 0x0B1E_0004,
    },
    // Exotic ten-cylinder screamer: ~1100..9500 rpm.
    EngineProfile {
        name: "Tempest",
        cylinders: 10,
        idle_hz: 92.0,
        limit_hz: 792.0,
        pipe_hz: 420.0,
        intake_hz: 1300.0,
        irregularity: 0.1,
        turbo: 0.0,
        seed: 0x0B1E_0005,
    },
    // Smooth twelve-cylinder tourer: ~700..7500 rpm, silky.
    EngineProfile {
        name: "Halcyon",
        cylinders: 12,
        idle_hz: 70.0,
        limit_hz: 750.0,
        pipe_hz: 260.0,
        intake_hz: 800.0,
        irregularity: 0.03,
        turbo: 0.0,
        seed: 0x0B1E_0006,
    },
    // Lumpy muscle eight: ~650..6200 rpm, heavy loping idle.
    EngineProfile {
        name: "Baron",
        cylinders: 8,
        idle_hz: 43.0,
        limit_hz: 413.0,
        pipe_hz: 110.0,
        intake_hz: 380.0,
        irregularity: 0.55,
        turbo: 0.0,
        seed: 0x0B1E_0007,
    },
];

/// Profile for a car index; out-of-range indices clamp to the last profile.
pub fn engine_profile(index: usize) -> &'static EngineProfile {
    &ENGINE_PROFILES[index.min(ENGINE_PROFILES.len() - 1)]
}

// ---------------------------------------------------------------------------
// DSP primitives
// ---------------------------------------------------------------------------

/// Small deterministic xorshift generator.
#[derive(Clone, Copy, Debug)]
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x1234_5678 } else { seed })
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in [0, 1).
    fn next_unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in [-1, 1).
    fn next_signed(&mut self) -> f32 {
        self.next_unit() * 2.0 - 1.0
    }
}

/// Stateless hash mapped to [-1, 1), used for per-cylinder patterns.
fn hash_signed(seed: u32, index: u32) -> f32 {
    let mut x = seed ^ index.wrapping_mul(0x9E37_79B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 23) as f32 - 1.0
}

/// Per-sample coefficient of a one-pole smoother with the given time constant.
fn one_pole_coef(tau_seconds: f32) -> f32 {
    1.0 - (-1.0 / (tau_seconds * SR)).exp()
}

/// Per-sample coefficient of a one-pole lowpass with the given cutoff.
fn lowpass_coef(cutoff_hz: f32) -> f32 {
    1.0 - (-TAU * cutoff_hz.clamp(1.0, SR * 0.45) / SR).exp()
}

/// Two-pole resonator with zeros at DC and Nyquist (roughly unity peak gain).
#[derive(Clone, Copy, Debug, Default)]
struct Resonator {
    a1: f32,
    a2: f32,
    b0: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Resonator {
    fn new(freq_hz: f32, bandwidth_hz: f32) -> Self {
        let r = (-PI * bandwidth_hz / SR).exp();
        let w = TAU * freq_hz.clamp(1.0, SR * 0.45) / SR;
        Self {
            a1: 2.0 * r * w.cos(),
            a2: -r * r,
            b0: (1.0 - r * r) * 0.5,
            ..Self::default()
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * (x - self.x2) + self.a1 * self.y1 + self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// First-order DC blocker.
#[derive(Clone, Copy, Debug, Default)]
struct DcBlocker {
    x1: f32,
    y1: f32,
}

impl DcBlocker {
    fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + 0.995 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

// ---------------------------------------------------------------------------
// Engine synthesis
// ---------------------------------------------------------------------------

/// Snapshot of the live engine parameters (already sanitized).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EngineParams {
    /// Output gain: volume when active and unmuted, otherwise 0.
    pub gain: f32,
    pub profile: usize,
    pub rpm: f32,
    pub throttle: f32,
    pub boost: f32,
}

impl EngineParams {
    /// Derive the decoder target from the game-facing resources.
    pub fn from_state(engine: &EngineAudio, settings: &AudioSettings) -> Self {
        let gain = if engine.active && !settings.muted {
            settings.volume
        } else {
            0.0
        };
        Self {
            gain,
            profile: engine.profile,
            rpm: engine.rpm,
            throttle: engine.throttle,
            boost: engine.boost,
        }
        .sanitized()
    }

    /// Clamp every field into its valid range; non-finite values become 0.
    pub fn sanitized(self) -> Self {
        let unit = |v: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        Self {
            gain: unit(self.gain),
            profile: self.profile.min(ENGINE_PROFILES.len() - 1),
            rpm: unit(self.rpm),
            throttle: unit(self.throttle),
            boost: unit(self.boost),
        }
    }
}

/// An `f32` stored as bits in an `AtomicU32`.
#[derive(Debug, Default)]
struct AtomicF32(AtomicU32);

impl AtomicF32 {
    fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    fn store(&self, value: f32) {
        let value = if value.is_finite() { value } else { 0.0 };
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }
}

/// Parameters shared between the ECS and the audio thread.
#[derive(Debug, Default)]
struct EngineShared {
    gain: AtomicF32,
    rpm: AtomicF32,
    throttle: AtomicF32,
    boost: AtomicF32,
    profile: AtomicUsize,
}

impl EngineShared {
    fn store(&self, params: EngineParams) {
        let p = params.sanitized();
        self.gain.store(p.gain);
        self.rpm.store(p.rpm);
        self.throttle.store(p.throttle);
        self.boost.store(p.boost);
        self.profile.store(p.profile, Ordering::Relaxed);
    }

    fn load(&self) -> EngineParams {
        EngineParams {
            gain: self.gain.load(),
            profile: self.profile.load(Ordering::Relaxed),
            rpm: self.rpm.load(),
            throttle: self.throttle.load(),
            boost: self.boost.load(),
        }
        .sanitized()
    }
}

/// Pure engine synthesizer: one call per output sample.
#[derive(Clone, Debug)]
pub struct EngineVoice {
    profile: usize,
    pipe: Resonator,
    intake: Resonator,
    dc: DcBlocker,
    rng: Rng,
    // Smoothed parameters.
    rpm: f32,
    throttle: f32,
    boost: f32,
    gain: f32,
    k_rpm: f32,
    k_throttle: f32,
    k_boost: f32,
    k_gain: f32,
    // Firing state.
    phase: f32,
    cylinder: u32,
    pulse_amp: f32,
    pulse_stretch: f32,
    // Turbo state.
    hiss_lp: f32,
    whistle_phase: f32,
}

impl Default for EngineVoice {
    fn default() -> Self {
        Self::new(0)
    }
}

impl EngineVoice {
    pub fn new(profile: usize) -> Self {
        let mut voice = Self {
            profile: usize::MAX,
            pipe: Resonator::default(),
            intake: Resonator::default(),
            dc: DcBlocker::default(),
            rng: Rng::new(0xE16_14E5),
            rpm: 0.0,
            throttle: 0.0,
            boost: 0.0,
            gain: 0.0,
            k_rpm: one_pole_coef(0.05),
            k_throttle: one_pole_coef(0.035),
            k_boost: one_pole_coef(0.09),
            k_gain: one_pole_coef(0.06),
            phase: 0.0,
            cylinder: 0,
            pulse_amp: 1.0,
            pulse_stretch: 1.0,
            hiss_lp: 0.0,
            whistle_phase: 0.0,
        };
        voice.set_profile(profile);
        voice
    }

    fn set_profile(&mut self, index: usize) {
        let index = index.min(ENGINE_PROFILES.len() - 1);
        if index == self.profile {
            return;
        }
        let p = &ENGINE_PROFILES[index];
        self.profile = index;
        self.pipe = Resonator::new(p.pipe_hz, p.pipe_hz * 0.35);
        self.intake = Resonator::new(p.intake_hz, p.intake_hz * 0.25);
        self.cylinder = 0;
        self.update_pulse_shape();
    }

    /// Per-cylinder firing amplitude and period stretch from the profile pattern.
    fn update_pulse_shape(&mut self) {
        let p = &ENGINE_PROFILES[self.profile];
        self.pulse_amp = 1.0 + p.irregularity * 0.6 * hash_signed(p.seed, self.cylinder);
        self.pulse_stretch =
            1.0 + p.irregularity * 0.3 * hash_signed(p.seed ^ 0xA5A5_A5A5, self.cylinder);
    }

    /// Render the next sample towards `target`. Output is always within [-1, 1].
    pub fn next_sample(&mut self, target: &EngineParams) -> f32 {
        self.set_profile(target.profile);
        self.rpm += (target.rpm - self.rpm) * self.k_rpm;
        self.throttle += (target.throttle - self.throttle) * self.k_throttle;
        self.boost += (target.boost - self.boost) * self.k_boost;
        self.gain += (target.gain - self.gain) * self.k_gain;

        let p = &ENGINE_PROFILES[self.profile];
        let (rpm, throttle, boost) = (self.rpm, self.throttle, self.boost);

        // Firing pulses: one per cylinder event, with per-cylinder unevenness.
        let freq = p.idle_hz + (p.limit_hz - p.idle_hz) * rpm;
        self.phase += freq / SR / self.pulse_stretch;
        if self.phase >= 1.0 {
            self.phase = (self.phase - 1.0).min(0.999);
            self.cylinder = (self.cylinder + 1) % u32::from(p.cylinders.max(1));
            self.update_pulse_shape();
        }
        let width = 0.55 - 0.3 * throttle;
        let pulse = if self.phase < width {
            (PI * self.phase / width).sin() * self.pulse_amp
        } else {
            0.0
        };

        let noise = self.rng.next_signed();
        let excite = pulse + noise * (0.04 + 0.08 * throttle);
        let pipe = self.pipe.process(excite);
        let intake = self.intake.process(excite);
        let mix = 0.5 * pulse + 1.2 * pipe + (0.3 + 0.6 * throttle) * intake;
        let drive = 1.0 + 2.0 * throttle + 0.8 * rpm;
        let body = (self.dc.process(mix) * drive).tanh();
        let level = 0.35 + 0.35 * throttle + 0.3 * rpm;

        // Airflow hiss (all cars under boost) and turbo whistle (turbo cars).
        self.hiss_lp += (noise - self.hiss_lp) * 0.15;
        let air = (noise - self.hiss_lp) * boost * (0.35 + 0.65 * p.turbo);
        let whistle_hz = 1800.0 + 5200.0 * rpm * (0.5 + 0.5 * boost);
        self.whistle_phase = (self.whistle_phase + whistle_hz / SR).fract();
        let whistle = (TAU * self.whistle_phase).sin() * p.turbo * boost * (0.3 + 0.7 * rpm);

        let out = (0.62 * body * level + 0.2 * air.clamp(-1.0, 1.0) + 0.13 * whistle) * self.gain;
        if out.is_finite() {
            out.clamp(-1.0, 1.0)
        } else {
            // Recover from any numeric blow-up instead of propagating NaN.
            *self = Self::new(self.profile);
            0.0
        }
    }
}

/// Continuous engine audio asset; its decoder follows the shared live parameters.
#[derive(Asset, TypePath)]
pub struct EngineSynth {
    shared: Arc<EngineShared>,
}

impl Decodable for EngineSynth {
    type Decoder = EngineDecoder;

    fn decoder(&self) -> Self::Decoder {
        EngineDecoder {
            shared: Arc::clone(&self.shared),
            voice: EngineVoice::new(self.shared.load().profile),
            target: self.shared.load(),
            refresh: 0,
        }
    }
}

/// How many samples between reads of the shared parameters.
const ENGINE_REFRESH_SAMPLES: u32 = 32;

/// Never-ending decoder for [`EngineSynth`].
pub struct EngineDecoder {
    shared: Arc<EngineShared>,
    voice: EngineVoice,
    target: EngineParams,
    refresh: u32,
}

impl Iterator for EngineDecoder {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.refresh == 0 {
            self.target = self.shared.load();
            self.refresh = ENGINE_REFRESH_SAMPLES;
        }
        self.refresh -= 1;
        Some(self.voice.next_sample(&self.target))
    }
}

impl Source for EngineDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        ChannelCount::new(1).expect("mono is non-zero")
    }

    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(SAMPLE_RATE).expect("sample rate is non-zero")
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// Link from the ECS to the engine decoder's shared parameters.
#[derive(Resource)]
struct EngineLink(Arc<EngineShared>);

fn spawn_engine(
    mut commands: Commands,
    mut assets: ResMut<Assets<EngineSynth>>,
    link: Res<EngineLink>,
) {
    let handle = assets.add(EngineSynth {
        shared: Arc::clone(&link.0),
    });
    // The decoder never ends, so `ONCE` plays forever. `LOOP` would wrap it in a
    // buffering repeat adapter that retains every rendered sample.
    commands.spawn((AudioPlayer(handle), PlaybackSettings::ONCE));
}

fn sync_engine(engine: Res<EngineAudio>, settings: Res<AudioSettings>, link: Res<EngineLink>) {
    link.0.store(EngineParams::from_state(&engine, &settings));
}

// ---------------------------------------------------------------------------
// Sound effect synthesis
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wave {
    Sine,
    Square,
    Triangle,
    Saw,
}

fn wave_sample(wave: Wave, phase: f32) -> f32 {
    match wave {
        Wave::Sine => (TAU * phase).sin(),
        Wave::Square => {
            if phase < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        Wave::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
        Wave::Saw => 2.0 * phase - 1.0,
    }
}

/// A pitched note with an exponential glide and a smooth decaying envelope.
#[derive(Clone, Copy, Debug)]
struct Tone {
    wave: Wave,
    start: f32,
    dur: f32,
    f0: f32,
    f1: f32,
    amp: f32,
    attack: f32,
    curve: f32,
    vibrato: (f32, f32),
    tremolo: (f32, f32),
}

impl Tone {
    fn new(wave: Wave, start: f32, dur: f32, freq: f32) -> Self {
        Self {
            wave,
            start,
            dur,
            f0: freq,
            f1: freq,
            amp: 0.5,
            attack: 0.003,
            curve: 2.0,
            vibrato: (0.0, 0.0),
            tremolo: (0.0, 0.0),
        }
    }
    fn glide(mut self, to: f32) -> Self {
        self.f1 = to;
        self
    }
    fn amp(mut self, amp: f32) -> Self {
        self.amp = amp;
        self
    }
    fn attack(mut self, seconds: f32) -> Self {
        self.attack = seconds;
        self
    }
    fn curve(mut self, curve: f32) -> Self {
        self.curve = curve;
        self
    }
    fn vibrato(mut self, rate: f32, depth: f32) -> Self {
        self.vibrato = (rate, depth);
        self
    }
    fn tremolo(mut self, rate: f32, depth: f32) -> Self {
        self.tremolo = (rate, depth);
        self
    }
}

/// A filtered noise burst with a sweeping lowpass cutoff.
#[derive(Clone, Copy, Debug)]
struct Noise {
    start: f32,
    dur: f32,
    cutoff0: f32,
    cutoff1: f32,
    amp: f32,
    attack: f32,
    curve: f32,
    /// Probability (0..1) that each short block is audible; 0 disables gating.
    crackle: f32,
}

impl Noise {
    fn new(start: f32, dur: f32) -> Self {
        Self {
            start,
            dur,
            cutoff0: 4000.0,
            cutoff1: 4000.0,
            amp: 0.5,
            attack: 0.002,
            curve: 2.0,
            crackle: 0.0,
        }
    }
    fn cutoff(mut self, from: f32, to: f32) -> Self {
        self.cutoff0 = from;
        self.cutoff1 = to;
        self
    }
    fn amp(mut self, amp: f32) -> Self {
        self.amp = amp;
        self
    }
    fn attack(mut self, seconds: f32) -> Self {
        self.attack = seconds;
        self
    }
    fn curve(mut self, curve: f32) -> Self {
        self.curve = curve;
        self
    }
    fn crackle(mut self, density: f32) -> Self {
        self.crackle = density;
        self
    }
}

/// Envelope: linear attack, then `(1 - x)^curve` decay reaching 0 at the end.
fn envelope(local_t: f32, dur: f32, attack: f32, curve: f32) -> f32 {
    let x = (local_t / dur).clamp(0.0, 1.0);
    let rise = if attack > 0.0 {
        (local_t / attack).min(1.0)
    } else {
        1.0
    };
    rise * (1.0 - x).powf(curve)
}

/// Mixing buffer for one sound effect.
struct SfxBuffer {
    samples: Vec<f32>,
}

impl SfxBuffer {
    fn new(seconds: f32) -> Self {
        Self {
            samples: vec![0.0; (seconds * SR).ceil() as usize],
        }
    }

    /// Sample range covered by an event, clipped to the buffer.
    fn span(&self, start: f32, dur: f32) -> std::ops::Range<usize> {
        let first = ((start * SR) as usize).min(self.samples.len());
        let last = (((start + dur) * SR) as usize).min(self.samples.len());
        first..last
    }

    fn tone(&mut self, t: Tone) -> &mut Self {
        let mut phase = 0.0f32;
        let ratio = t.f1 / t.f0;
        for i in self.span(t.start, t.dur) {
            let lt = i as f32 / SR - t.start;
            let x = lt / t.dur;
            let vib = 1.0 + t.vibrato.1 * (TAU * t.vibrato.0 * lt).sin();
            let freq = t.f0 * ratio.powf(x) * vib;
            phase = (phase + freq / SR).fract();
            let trem = 1.0 - t.tremolo.1 * 0.5 * (1.0 - (TAU * t.tremolo.0 * lt).cos());
            let env = envelope(lt, t.dur, t.attack, t.curve) * trem;
            self.samples[i] += wave_sample(t.wave, phase) * env * t.amp;
        }
        self
    }

    fn noise(&mut self, n: Noise, rng: &mut Rng) -> &mut Self {
        const BLOCK: usize = 160;
        let mut lp = 0.0f32;
        let mut gate = 1.0f32;
        for (k, i) in self.span(n.start, n.dur).enumerate() {
            let lt = i as f32 / SR - n.start;
            let x = lt / n.dur;
            let cutoff = n.cutoff0 + (n.cutoff1 - n.cutoff0) * x;
            lp += (rng.next_signed() - lp) * lowpass_coef(cutoff);
            if n.crackle > 0.0 && k % BLOCK == 0 {
                gate = if rng.next_unit() < n.crackle {
                    0.5 + rng.next_unit()
                } else {
                    0.0
                };
            }
            let env = envelope(lt, n.dur, n.attack, n.curve);
            // Lowpassed noise loses level; compensate a little for low cutoffs.
            self.samples[i] += lp * env * n.amp * gate * 1.8;
        }
        self
    }

    /// Soft-clip, then normalize the peak to `gain` (<= 1).
    fn finish(&mut self, gain: f32) -> Vec<f32> {
        let mut out: Vec<f32> = self.samples.iter().map(|s| s.tanh()).collect();
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        if peak > 1e-6 {
            let scale = gain.clamp(0.0, 1.0) / peak;
            out.iter_mut().for_each(|s| *s *= scale);
        }
        out
    }
}

/// One pre-rendered buffer per slot; `BoostPop` tiers get their own slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SfxSlot {
    CountdownTick,
    CountdownGo,
    SectorChime,
    LapComplete,
    FinishSlam,
    GearShift,
    BoostPop1,
    BoostPop2,
    BoostPop3,
    NitroIgnite,
    PadBoost,
    Bump,
    Spinout,
    ItemPickup,
    Thumper,
    Slick,
    StaticBurst,
    BatteringBoost,
    AllTerrain,
    Phantom,
    DriftSurge,
    Jammer,
    MenuMove,
    MenuSelect,
    Coin,
    Purchase,
    Denied,
}

const SFX_SLOTS: [SfxSlot; 27] = [
    SfxSlot::CountdownTick,
    SfxSlot::CountdownGo,
    SfxSlot::SectorChime,
    SfxSlot::LapComplete,
    SfxSlot::FinishSlam,
    SfxSlot::GearShift,
    SfxSlot::BoostPop1,
    SfxSlot::BoostPop2,
    SfxSlot::BoostPop3,
    SfxSlot::NitroIgnite,
    SfxSlot::PadBoost,
    SfxSlot::Bump,
    SfxSlot::Spinout,
    SfxSlot::ItemPickup,
    SfxSlot::Thumper,
    SfxSlot::Slick,
    SfxSlot::StaticBurst,
    SfxSlot::BatteringBoost,
    SfxSlot::AllTerrain,
    SfxSlot::Phantom,
    SfxSlot::DriftSurge,
    SfxSlot::Jammer,
    SfxSlot::MenuMove,
    SfxSlot::MenuSelect,
    SfxSlot::Coin,
    SfxSlot::Purchase,
    SfxSlot::Denied,
];

/// Map a request to its buffer slot and a volume scale (before settings volume).
fn sfx_slot(sfx: Sfx) -> (SfxSlot, f32) {
    match sfx {
        Sfx::CountdownTick => (SfxSlot::CountdownTick, 1.0),
        Sfx::CountdownGo => (SfxSlot::CountdownGo, 1.0),
        Sfx::SectorChime => (SfxSlot::SectorChime, 1.0),
        Sfx::LapComplete => (SfxSlot::LapComplete, 1.0),
        Sfx::FinishSlam => (SfxSlot::FinishSlam, 1.0),
        Sfx::GearShift => (SfxSlot::GearShift, 1.0),
        Sfx::BoostPop { tier } => match tier {
            0 | 1 => (SfxSlot::BoostPop1, 1.0),
            2 => (SfxSlot::BoostPop2, 1.0),
            _ => (SfxSlot::BoostPop3, 1.0),
        },
        Sfx::NitroIgnite => (SfxSlot::NitroIgnite, 1.0),
        Sfx::PadBoost => (SfxSlot::PadBoost, 1.0),
        Sfx::Bump { strength } => {
            let s = if strength.is_finite() {
                strength.clamp(0.0, 1.0)
            } else {
                0.0
            };
            (SfxSlot::Bump, 0.2 + 0.8 * s)
        }
        Sfx::Spinout => (SfxSlot::Spinout, 1.0),
        Sfx::ItemPickup => (SfxSlot::ItemPickup, 1.0),
        Sfx::Thumper => (SfxSlot::Thumper, 1.0),
        Sfx::Slick => (SfxSlot::Slick, 1.0),
        Sfx::StaticBurst => (SfxSlot::StaticBurst, 1.0),
        Sfx::BatteringBoost => (SfxSlot::BatteringBoost, 1.0),
        Sfx::AllTerrain => (SfxSlot::AllTerrain, 1.0),
        Sfx::Phantom => (SfxSlot::Phantom, 1.0),
        Sfx::DriftSurge => (SfxSlot::DriftSurge, 1.0),
        Sfx::Jammer => (SfxSlot::Jammer, 1.0),
        Sfx::MenuMove => (SfxSlot::MenuMove, 1.0),
        Sfx::MenuSelect => (SfxSlot::MenuSelect, 1.0),
        Sfx::Coin => (SfxSlot::Coin, 1.0),
        Sfx::Purchase => (SfxSlot::Purchase, 1.0),
        Sfx::Denied => (SfxSlot::Denied, 1.0),
    }
}

fn boost_pop(tier: u8, rng: &mut Rng) -> Vec<f32> {
    use Wave::*;
    let t = f32::from(tier.clamp(1, 3));
    let dur = 0.14 + 0.07 * t;
    let f0 = 260.0 * 1.26f32.powf(t - 1.0);
    SfxBuffer::new(dur + 0.01)
        .tone(Tone::new(Square, 0.0, dur, f0).glide(f0 * 2.5).amp(0.4))
        .tone(Tone::new(Sine, 0.0, dur, f0 * 2.0).glide(f0 * 5.0).amp(0.3))
        .noise(Noise::new(0.0, 0.05).cutoff(5000.0, 2000.0).amp(0.5), rng)
        .finish(0.5 + 0.1 * t)
}

/// Synthesize one sound effect buffer. Deterministic for a given slot.
fn render_sfx(slot: SfxSlot) -> Vec<f32> {
    use Wave::*;
    let mut rng = Rng::new(0x9E37_79B9 ^ (slot as u32 + 1).wrapping_mul(0x85EB_CA6B));
    let rng = &mut rng;
    match slot {
        SfxSlot::CountdownTick => SfxBuffer::new(0.16)
            .tone(Tone::new(Square, 0.0, 0.14, 660.0).amp(0.6))
            .tone(Tone::new(Sine, 0.0, 0.14, 1320.0).amp(0.3))
            .finish(0.55),
        SfxSlot::CountdownGo => SfxBuffer::new(0.56)
            .tone(Tone::new(Square, 0.0, 0.55, 880.0).amp(0.4).curve(1.2))
            .tone(Tone::new(Square, 0.0, 0.55, 1318.5).amp(0.3).curve(1.2))
            .tone(Tone::new(Sine, 0.0, 0.55, 440.0).amp(0.3).curve(1.2))
            .finish(0.7),
        SfxSlot::SectorChime => SfxBuffer::new(0.4)
            .tone(Tone::new(Sine, 0.0, 0.2, 1046.5).amp(0.6))
            .tone(Tone::new(Sine, 0.08, 0.3, 1568.0).amp(0.6))
            .tone(Tone::new(Triangle, 0.08, 0.25, 2093.0).amp(0.2))
            .finish(0.5),
        SfxSlot::LapComplete => {
            let mut b = SfxBuffer::new(0.6);
            for (i, f) in [784.0, 988.0, 1175.0, 1568.0].into_iter().enumerate() {
                let start = i as f32 * 0.08;
                let dur = if i == 3 { 0.34 } else { 0.22 };
                b.tone(Tone::new(Triangle, start, dur, f).amp(0.5));
                b.tone(Tone::new(Square, start, dur, f * 0.5).amp(0.12));
            }
            b.finish(0.65)
        }
        SfxSlot::FinishSlam => {
            let mut b = SfxBuffer::new(1.2);
            b.noise(Noise::new(0.0, 0.5).cutoff(3000.0, 300.0).amp(1.0).curve(3.0), rng)
                .tone(Tone::new(Saw, 0.0, 0.9, 110.0).glide(55.0).amp(0.5))
                .tone(Tone::new(Sine, 0.0, 0.6, 65.0).amp(0.8));
            for f in [523.3, 659.3, 784.0] {
                b.tone(
                    Tone::new(Square, 0.12, 1.06, f)
                        .amp(0.22)
                        .curve(1.5)
                        .vibrato(5.5, 0.006),
                );
            }
            b.finish(0.85)
        }
        SfxSlot::GearShift => SfxBuffer::new(0.1)
            .noise(Noise::new(0.0, 0.03).cutoff(6000.0, 2000.0).amp(0.6), rng)
            .tone(Tone::new(Sine, 0.0, 0.09, 160.0).glide(90.0).amp(0.8))
            .finish(0.45),
        SfxSlot::BoostPop1 => boost_pop(1, rng),
        SfxSlot::BoostPop2 => boost_pop(2, rng),
        SfxSlot::BoostPop3 => boost_pop(3, rng),
        SfxSlot::NitroIgnite => SfxBuffer::new(0.7)
            .noise(
                Noise::new(0.0, 0.7)
                    .cutoff(400.0, 6000.0)
                    .amp(0.8)
                    .attack(0.08)
                    .curve(1.5),
                rng,
            )
            .tone(Tone::new(Saw, 0.0, 0.6, 70.0).glide(140.0).amp(0.4))
            .tone(Tone::new(Sine, 0.1, 0.5, 1200.0).glide(3200.0).amp(0.12).attack(0.05))
            .finish(0.75),
        SfxSlot::PadBoost => SfxBuffer::new(0.3)
            .tone(Tone::new(Square, 0.0, 0.3, 400.0).glide(1600.0).amp(0.4).vibrato(30.0, 0.02))
            .tone(Tone::new(Sine, 0.0, 0.3, 800.0).glide(3200.0).amp(0.25))
            .finish(0.6),
        SfxSlot::Bump => SfxBuffer::new(0.18)
            .noise(Noise::new(0.0, 0.18).cutoff(900.0, 200.0).amp(0.9).curve(3.0), rng)
            .tone(Tone::new(Sine, 0.0, 0.18, 95.0).glide(55.0).amp(1.0))
            .finish(0.8),
        SfxSlot::Spinout => SfxBuffer::new(0.75)
            .tone(Tone::new(Saw, 0.0, 0.75, 900.0).glide(140.0).amp(0.35).vibrato(9.0, 0.06))
            .tone(Tone::new(Square, 0.0, 0.7, 450.0).glide(70.0).amp(0.2))
            .noise(Noise::new(0.0, 0.7).cutoff(2500.0, 800.0).amp(0.4), rng)
            .finish(0.65),
        SfxSlot::ItemPickup => SfxBuffer::new(0.22)
            .tone(Tone::new(Sine, 0.0, 0.08, 988.0).amp(0.6))
            .tone(Tone::new(Sine, 0.06, 0.16, 1480.0).amp(0.6))
            .tone(Tone::new(Triangle, 0.06, 0.16, 1976.0).amp(0.3))
            .finish(0.55),
        SfxSlot::Thumper => SfxBuffer::new(0.55)
            .tone(Tone::new(Sine, 0.0, 0.55, 75.0).glide(38.0).amp(1.0).curve(2.5))
            .noise(Noise::new(0.0, 0.4).cutoff(600.0, 120.0).amp(0.8).curve(3.0), rng)
            .tone(Tone::new(Square, 0.0, 0.3, 150.0).glide(60.0).amp(0.2))
            .finish(0.85),
        SfxSlot::Slick => SfxBuffer::new(0.4)
            .tone(Tone::new(Sine, 0.0, 0.4, 650.0).glide(180.0).amp(0.6).vibrato(14.0, 0.08))
            .tone(Tone::new(Triangle, 0.0, 0.4, 1300.0).glide(360.0).amp(0.2))
            .noise(Noise::new(0.0, 0.3).cutoff(1500.0, 600.0).amp(0.15), rng)
            .finish(0.6),
        SfxSlot::StaticBurst => SfxBuffer::new(0.5)
            .noise(Noise::new(0.0, 0.5).cutoff(7000.0, 2500.0).amp(1.0).crackle(0.45).curve(1.2), rng)
            .tone(Tone::new(Square, 0.0, 0.5, 2400.0).glide(1800.0).amp(0.08).tremolo(40.0, 1.0))
            .finish(0.6),
        SfxSlot::BatteringBoost => SfxBuffer::new(0.6)
            .tone(Tone::new(Saw, 0.0, 0.6, 80.0).glide(220.0).amp(0.5).curve(1.2))
            .noise(Noise::new(0.0, 0.6).cutoff(300.0, 1800.0).amp(0.6).attack(0.04), rng)
            .tone(Tone::new(Sine, 0.0, 0.5, 40.0).glide(70.0).amp(0.6))
            .finish(0.8),
        SfxSlot::AllTerrain => {
            let mut b = SfxBuffer::new(0.36);
            for i in 0..3 {
                let start = i as f32 * 0.12;
                b.tone(Tone::new(Square, start, 0.1, 220.0).glide(160.0).amp(0.5));
                b.noise(Noise::new(start, 0.06).cutoff(1200.0, 400.0).amp(0.3), rng);
            }
            b.finish(0.65)
        }
        SfxSlot::Phantom => SfxBuffer::new(0.75)
            .tone(
                Tone::new(Sine, 0.0, 0.75, 1200.0)
                    .glide(600.0)
                    .amp(0.45)
                    .attack(0.12)
                    .curve(1.5)
                    .tremolo(7.0, 0.6),
            )
            .tone(Tone::new(Sine, 0.0, 0.75, 1212.0).glide(606.0).amp(0.45).attack(0.12).curve(1.5))
            .tone(Tone::new(Triangle, 0.0, 0.7, 2400.0).glide(1200.0).amp(0.1).attack(0.1))
            .finish(0.55),
        SfxSlot::DriftSurge => SfxBuffer::new(0.42)
            .tone(Tone::new(Triangle, 0.0, 0.42, 300.0).glide(900.0).amp(0.5))
            .tone(Tone::new(Square, 0.0, 0.42, 150.0).glide(450.0).amp(0.2))
            .noise(Noise::new(0.0, 0.42).cutoff(1000.0, 5000.0).amp(0.3), rng)
            .finish(0.65),
        SfxSlot::Jammer => {
            let mut b = SfxBuffer::new(0.6);
            for i in 0..8 {
                let f = if i % 2 == 0 { 440.0 } else { 620.0 };
                b.tone(Tone::new(Square, i as f32 * 0.075, 0.075, f).amp(0.35).curve(0.5));
            }
            b.noise(Noise::new(0.0, 0.6).cutoff(4000.0, 4000.0).amp(0.25).crackle(0.3), rng);
            b.finish(0.6)
        }
        SfxSlot::MenuMove => SfxBuffer::new(0.05)
            .tone(Tone::new(Square, 0.0, 0.05, 880.0).amp(0.4).curve(1.0))
            .finish(0.35),
        SfxSlot::MenuSelect => SfxBuffer::new(0.16)
            .tone(Tone::new(Square, 0.0, 0.06, 660.0).amp(0.4))
            .tone(Tone::new(Square, 0.05, 0.11, 1320.0).amp(0.4))
            .finish(0.45),
        SfxSlot::Coin => SfxBuffer::new(0.32)
            .tone(Tone::new(Triangle, 0.0, 0.07, 1046.5).amp(0.6))
            .tone(Tone::new(Triangle, 0.06, 0.26, 1568.0).amp(0.6).curve(1.5))
            .tone(Tone::new(Sine, 0.06, 0.2, 3136.0).amp(0.15))
            .finish(0.5),
        SfxSlot::Purchase => {
            let mut b = SfxBuffer::new(0.55);
            for (i, f) in [523.3, 659.3, 784.0, 1046.5].into_iter().enumerate() {
                b.tone(Tone::new(Triangle, i as f32 * 0.07, 0.3, f).amp(0.4));
            }
            b.tone(Tone::new(Sine, 0.21, 0.34, 2093.0).amp(0.15).tremolo(12.0, 0.5));
            b.finish(0.6)
        }
        SfxSlot::Denied => SfxBuffer::new(0.32)
            .tone(Tone::new(Square, 0.0, 0.12, 165.0).amp(0.5).curve(0.8))
            .tone(Tone::new(Square, 0.16, 0.14, 155.0).amp(0.5).curve(0.8))
            .tone(Tone::new(Saw, 0.0, 0.3, 82.0).amp(0.2))
            .finish(0.55),
    }
}

/// A pre-rendered mono sound effect.
#[derive(Asset, TypePath, Clone)]
pub struct SfxClip {
    samples: Arc<[f32]>,
}

impl Decodable for SfxClip {
    type Decoder = SfxDecoder;

    fn decoder(&self) -> Self::Decoder {
        SfxDecoder {
            samples: Arc::clone(&self.samples),
            position: 0,
        }
    }
}

/// Finite decoder over an [`SfxClip`] buffer.
pub struct SfxDecoder {
    samples: Arc<[f32]>,
    position: usize,
}

impl Iterator for SfxDecoder {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.samples.get(self.position).copied();
        self.position += 1;
        sample
    }
}

impl Source for SfxDecoder {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.samples.len().saturating_sub(self.position))
    }

    fn channels(&self) -> ChannelCount {
        ChannelCount::new(1).expect("mono is non-zero")
    }

    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(SAMPLE_RATE).expect("sample rate is non-zero")
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(
            self.samples.len() as f64 / f64::from(SAMPLE_RATE),
        ))
    }
}

/// Handles of every pre-rendered effect, indexed by `SfxSlot as usize`.
#[derive(Resource)]
struct SfxBank(Vec<Handle<SfxClip>>);

fn build_sfx_bank(mut commands: Commands, mut assets: ResMut<Assets<SfxClip>>) {
    let handles = SFX_SLOTS
        .iter()
        .map(|&slot| {
            assets.add(SfxClip {
                samples: render_sfx(slot).into(),
            })
        })
        .collect();
    commands.insert_resource(SfxBank(handles));
}

/// Plays requested effects. Duplicate requests in one frame merge (loudest wins).
fn play_sfx(
    mut commands: Commands,
    mut requests: MessageReader<PlaySfx>,
    settings: Res<AudioSettings>,
    bank: Option<Res<SfxBank>>,
) {
    let (Some(bank), false) = (bank, settings.muted) else {
        requests.clear();
        return;
    };
    let mut volumes = [0.0f32; SFX_SLOTS.len()];
    for PlaySfx(sfx) in requests.read() {
        let (slot, scale) = sfx_slot(*sfx);
        let v = &mut volumes[slot as usize];
        *v = v.max(scale);
    }
    let master = if settings.volume.is_finite() {
        settings.volume.clamp(0.0, 1.0)
    } else {
        0.0
    };
    for (handle, scale) in bank.0.iter().zip(volumes) {
        if scale > 0.0 && master > 0.0 {
            commands.spawn((
                AudioPlayer(handle.clone()),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(master * scale)),
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    fn render_engine(voice: &mut EngineVoice, target: EngineParams, n: usize) -> Vec<f32> {
        (0..n).map(|_| voice.next_sample(&target)).collect()
    }

    #[test]
    fn slot_table_matches_enum_order() {
        for (i, slot) in SFX_SLOTS.iter().enumerate() {
            assert_eq!(*slot as usize, i);
        }
    }

    #[test]
    fn every_sfx_is_finite_bounded_and_short() {
        for slot in SFX_SLOTS {
            let samples = render_sfx(slot);
            assert!(!samples.is_empty(), "{slot:?} is empty");
            assert!(samples.len() <= (1.25 * SR) as usize, "{slot:?} too long");
            assert!(samples.iter().all(|s| s.is_finite()), "{slot:?} not finite");
            let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(peak <= 1.0 && peak > 0.05, "{slot:?} peak {peak}");
        }
    }

    #[test]
    fn sfx_rendering_is_deterministic() {
        for slot in SFX_SLOTS {
            assert_eq!(render_sfx(slot), render_sfx(slot), "{slot:?}");
        }
    }

    #[test]
    fn boost_pop_tiers_grow_longer_and_clamp() {
        assert_eq!(sfx_slot(Sfx::BoostPop { tier: 0 }).0, SfxSlot::BoostPop1);
        assert_eq!(sfx_slot(Sfx::BoostPop { tier: 9 }).0, SfxSlot::BoostPop3);
        let lens: Vec<usize> = [SfxSlot::BoostPop1, SfxSlot::BoostPop2, SfxSlot::BoostPop3]
            .into_iter()
            .map(|s| render_sfx(s).len())
            .collect();
        assert!(lens[0] < lens[1] && lens[1] < lens[2]);
    }

    #[test]
    fn bump_volume_scales_with_strength() {
        let soft = sfx_slot(Sfx::Bump { strength: 0.0 }).1;
        let hard = sfx_slot(Sfx::Bump { strength: 1.0 }).1;
        let wild = sfx_slot(Sfx::Bump { strength: f32::NAN }).1;
        assert!(soft < hard);
        assert!((hard - 1.0).abs() < 1e-6);
        assert!(wild.is_finite() && wild <= hard);
    }

    #[test]
    fn rng_is_deterministic_and_in_range() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            let x = a.next_signed();
            assert_eq!(x, b.next_signed());
            assert!((-1.0..1.0).contains(&x));
        }
        assert!((0..64).all(|i| (-1.0..1.0).contains(&hash_signed(7, i))));
    }

    #[test]
    fn params_are_sanitized() {
        let p = EngineParams {
            gain: f32::NAN,
            profile: 99,
            rpm: 3.0,
            throttle: -1.0,
            boost: f32::INFINITY,
        }
        .sanitized();
        assert_eq!(p.gain, 0.0);
        assert_eq!(p.profile, ENGINE_PROFILES.len() - 1);
        assert_eq!((p.rpm, p.throttle, p.boost), (1.0, 0.0, 0.0));
    }

    #[test]
    fn shared_params_round_trip() {
        let shared = EngineShared::default();
        let p = EngineParams {
            gain: 0.7,
            profile: 3,
            rpm: 0.25,
            throttle: 0.5,
            boost: 1.0,
        };
        shared.store(p);
        assert_eq!(shared.load(), p);
    }

    #[test]
    fn muted_or_inactive_engine_has_zero_gain() {
        let engine = EngineAudio {
            active: true,
            profile: 2,
            rpm: 0.5,
            throttle: 1.0,
            boost: 0.0,
        };
        let muted = AudioSettings {
            muted: true,
            volume: 0.7,
        };
        assert_eq!(EngineParams::from_state(&engine, &muted).gain, 0.0);
        let idle = EngineAudio {
            active: false,
            ..engine
        };
        assert_eq!(
            EngineParams::from_state(&idle, &AudioSettings::default()).gain,
            0.0
        );
        assert!(EngineParams::from_state(&engine, &AudioSettings::default()).gain > 0.6);
    }

    #[test]
    fn engine_is_finite_and_bounded_for_every_profile() {
        for profile in 0..ENGINE_PROFILES.len() {
            let mut voice = EngineVoice::new(profile);
            for (rpm, throttle, boost) in [(0.0, 0.0, 0.0), (0.5, 1.0, 0.5), (1.0, 1.0, 1.0)] {
                let target = EngineParams {
                    gain: 1.0,
                    profile,
                    rpm,
                    throttle,
                    boost,
                };
                let out = render_engine(&mut voice, target, 8_000);
                assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
                assert!(rms(&out[4_000..]) > 0.01, "profile {profile} silent");
            }
        }
    }

    #[test]
    fn engine_is_deterministic() {
        let target = EngineParams {
            gain: 0.8,
            profile: 6,
            rpm: 0.4,
            throttle: 0.7,
            boost: 0.3,
        };
        let a = render_engine(&mut EngineVoice::new(6), target, 4_000);
        let b = render_engine(&mut EngineVoice::new(6), target, 4_000);
        assert_eq!(a, b);
    }

    #[test]
    fn engine_fades_out_when_gain_drops() {
        let mut voice = EngineVoice::new(1);
        let on = EngineParams {
            gain: 1.0,
            profile: 1,
            rpm: 0.6,
            throttle: 1.0,
            boost: 0.0,
        };
        render_engine(&mut voice, on, 10_000);
        let off = EngineParams { gain: 0.0, ..on };
        let tail = render_engine(&mut voice, off, 22_050);
        assert!(rms(&tail[20_000..]) < 1e-3);
    }

    #[test]
    fn throttle_makes_engine_louder() {
        let base = EngineParams {
            gain: 1.0,
            profile: 0,
            rpm: 0.5,
            throttle: 0.0,
            boost: 0.0,
        };
        let quiet = render_engine(&mut EngineVoice::new(0), base, 16_000);
        let loud = render_engine(
            &mut EngineVoice::new(0),
            EngineParams {
                throttle: 1.0,
                ..base
            },
            16_000,
        );
        assert!(rms(&loud[8_000..]) > rms(&quiet[8_000..]));
    }

    #[test]
    fn profiles_are_well_formed() {
        for p in &ENGINE_PROFILES {
            assert!(p.cylinders > 0);
            assert!(p.idle_hz > 0.0 && p.idle_hz < p.limit_hz);
            assert!(p.limit_hz < SR * 0.25);
            assert!((0.0..=1.0).contains(&p.irregularity));
            assert!((0.0..=1.0).contains(&p.turbo));
        }
        assert_eq!(engine_profile(42).name, "Baron");
    }

    #[test]
    fn sfx_decoder_plays_buffer_then_ends() {
        let clip = SfxClip {
            samples: vec![0.1, 0.2, 0.3].into(),
        };
        let collected: Vec<f32> = clip.decoder().collect();
        assert_eq!(collected, vec![0.1, 0.2, 0.3]);
    }
}
