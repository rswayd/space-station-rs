//! What BYOND's `sound` datum does to a sound on the client, as a [`Source`] over the decoded
//! file ([`ProcessedSound`]):
//!
//! - `sound.pitch` ([`PitchShift`]): the pitch moves, the length stays. Two read heads sweep a
//!   short delay line at the pitch ratio, each faded in and out (sin²) so that the jump back of
//!   one is under the other's peak — a granular shifter, cheap enough for every sound.
//! - `sound.environment` ([`Reverb`]): BYOND plays sounds through FMOD's reverb presets; each
//!   `SOUND_ENVIRONMENT_*` is its preset's decay time, high cut, late delay and wet level
//!   ([`preset`]) on a small Schroeder reverb (four damped combs, two allpasses), added to the
//!   dry sound. Space and underwater (`SOUND_AREA_SPACE`) are a long, dark, loud tail.
//!
//! The effects run on the audio thread as the sound plays; a sound without either plays through
//! untouched.
// shortcut: the presets keep their decay, high cut, late delay and wet level only (no early
// reflections, density, diffusion or low shelf; drugged/dizzy/psychotic have no pitch
// modulation), and the pitch shifter's grains colour long notes slightly; upgrade to a phase
// vocoder or an FMOD-grade reverb if players notice.

use std::f32::consts::PI;
use std::time::Duration;

use bevy::audio::{AudioSource, ChannelCount, Decodable, Sample, SampleRate, Source};
use bevy::prelude::*;
use ss13_shared::protocol::SoundFx;

/// A sound file and what its `sound` datum does to it.
#[derive(Asset, TypePath, Clone, Debug)]
pub struct ProcessedSound {
    pub source: AudioSource,
    pub fx: SoundFx,
}

impl Decodable for ProcessedSound {
    type Decoder = Effects<<AudioSource as Decodable>::Decoder>;

    fn decoder(&self) -> Self::Decoder {
        Effects::new(self.source.decoder(), self.fx)
    }
}

/// Seconds of one pitch-shift grain.
const GRAIN_SECS: f32 = 0.06;
/// A reverb tail never rings longer than this after the sound ends (the hangar's 10 s would).
const MAX_TAIL_SECS: f32 = 4.0;

/// A reverb preset: FMOD's `FMOD_PRESET_*` (BYOND's environments are their indices).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    pub decay_secs: f32,
    /// `HighCut`: the tail's damping.
    pub high_cut_hz: f32,
    /// `LateDelay`.
    pub late_delay_ms: f32,
    /// `WetLevel`, dB against the dry sound.
    pub wet_db: f32,
}

const fn p(decay_ms: f32, high_cut_hz: f32, late_delay_ms: f32, wet_db: f32) -> Preset {
    Preset {
        decay_secs: decay_ms / 1000.0,
        high_cut_hz,
        late_delay_ms,
        wet_db,
    }
}

/// `SOUND_ENVIRONMENT_GENERIC` (0) .. `SOUND_ENVIRONMENT_PSYCHOTIC` (25).
const PRESETS: [Preset; 26] = [
    p(1500.0, 14500.0, 11.0, -8.0),  // generic
    p(170.0, 160.0, 2.0, -7.8),      // padded cell
    p(400.0, 6050.0, 3.0, -9.4),     // room
    p(1500.0, 2900.0, 11.0, 0.5),    // bathroom
    p(500.0, 160.0, 4.0, -19.0),     // living room
    p(2300.0, 7800.0, 17.0, -8.5),   // stone room
    p(4300.0, 5850.0, 30.0, -11.7),  // auditorium
    p(3900.0, 5650.0, 29.0, -9.8),   // concert hall
    p(2900.0, 20000.0, 22.0, -11.3), // cave
    p(7200.0, 4500.0, 30.0, -9.6),   // arena
    p(10000.0, 3400.0, 30.0, -7.4),  // hangar
    p(300.0, 500.0, 30.0, -24.0),    // carpeted hallway
    p(1500.0, 7800.0, 11.0, -5.5),   // hallway
    p(270.0, 9000.0, 20.0, -6.0),    // stone corridor
    p(1500.0, 8300.0, 11.0, -9.8),   // alley
    p(1500.0, 760.0, 88.0, -12.3),   // forest
    p(1500.0, 4050.0, 11.0, -26.0),  // city
    p(1500.0, 1220.0, 100.0, -24.0), // mountains
    p(1500.0, 3400.0, 25.0, -5.0),   // quarry
    p(1500.0, 1670.0, 100.0, -28.0), // plain
    p(1700.0, 20000.0, 12.0, -19.5), // parking lot (the station)
    p(2800.0, 3400.0, 21.0, 1.2),    // sewer pipe
    p(1500.0, 500.0, 11.0, 7.0),     // underwater (space)
    p(8390.0, 20000.0, 30.0, -4.0),  // drugged
    p(17230.0, 20000.0, 30.0, -6.0), // dizzy
    p(7560.0, 20000.0, 30.0, -4.0),  // psychotic
];

/// The preset of a BYOND environment; `None` for `SOUND_ENVIRONMENT_NONE` (or out of range).
pub fn preset(environment: i8) -> Option<Preset> {
    usize::try_from(environment)
        .ok()
        .and_then(|i| PRESETS.get(i))
        .copied()
}

/// The pitch shifter of one sound (all its channels).
#[derive(Clone, Debug)]
pub struct PitchShift {
    ratio: f32,
    /// Grain length, frames.
    window: f32,
    channels: usize,
    /// Ring of input frames, interleaved.
    ring: Vec<f32>,
    frames: usize,
    write: usize,
    phase: f32,
}

impl PitchShift {
    pub fn new(ratio: f32, channels: usize, rate: u32) -> Self {
        let window = (rate as f32 * GRAIN_SECS).max(16.0).floor();
        let frames = window as usize + 2;
        Self {
            ratio,
            window,
            channels,
            ring: vec![0.0; frames * channels],
            frames,
            write: 0,
            phase: 0.0,
        }
    }

    /// Frames of delay the shifter adds at most (its tail).
    pub fn tail_frames(&self) -> usize {
        self.window as usize
    }

    /// One input frame in, one output frame added into `out`.
    pub fn process(&mut self, input: &[f32], out: &mut [f32]) {
        let ch = self.channels;
        let w = self.write;
        self.ring[w * ch..(w + 1) * ch].copy_from_slice(input);
        let n = self.frames as f32;
        for offset in [0.0, 0.5] {
            let phase = (self.phase + offset).fract();
            // Read heads move at `ratio` frames per frame: the delay grows by `1 - ratio`.
            let pos = (w as f32 - phase * self.window).rem_euclid(n);
            let i0 = pos.floor() as usize % self.frames;
            let i1 = (i0 + 1) % self.frames;
            let frac = pos - pos.floor();
            let gain = (PI * phase).sin().powi(2);
            for (c, o) in out.iter_mut().enumerate().take(ch) {
                let a = self.ring[i0 * ch + c];
                let b = self.ring[i1 * ch + c];
                *o += gain * (a + (b - a) * frac);
            }
        }
        self.write = (w + 1) % self.frames;
        self.phase = (self.phase + (1.0 - self.ratio) / self.window).rem_euclid(1.0);
    }
}

/// A damped feedback comb.
#[derive(Clone, Debug)]
struct Comb {
    buf: Vec<f32>,
    i: usize,
    feedback: f32,
    damp: f32,
    store: f32,
}

impl Comb {
    fn process(&mut self, x: f32) -> f32 {
        let out = self.buf[self.i];
        self.store = out * (1.0 - self.damp) + self.store * self.damp;
        self.buf[self.i] = x + self.store * self.feedback;
        self.i = (self.i + 1) % self.buf.len();
        out
    }
}

#[derive(Clone, Debug)]
struct Allpass {
    buf: Vec<f32>,
    i: usize,
}

impl Allpass {
    fn process(&mut self, x: f32) -> f32 {
        let b = self.buf[self.i];
        self.buf[self.i] = x + b * 0.5;
        self.i = (self.i + 1) % self.buf.len();
        b - x
    }
}

/// Freeverb's comb and allpass lengths at 44.1 kHz.
const COMBS: [usize; 4] = [1116, 1188, 1277, 1356];
const ALLPASSES: [usize; 2] = [556, 441];

/// A mono reverb fed the mix of a sound's channels, its tail added to each.
#[derive(Clone, Debug)]
pub struct Reverb {
    predelay: Vec<f32>,
    pre_i: usize,
    combs: Vec<Comb>,
    allpasses: Vec<Allpass>,
    input_gain: f32,
    wet: f32,
    tail: usize,
}

impl Reverb {
    pub fn new(preset: Preset, rate: u32) -> Self {
        let scale = rate as f32 / 44100.0;
        let len = |n: usize| ((n as f32 * scale) as usize).max(1);
        let decay = preset.decay_secs.max(0.01);
        let damp = (-2.0 * PI * preset.high_cut_hz / rate as f32)
            .exp()
            .clamp(0.0, 0.99);
        let combs: Vec<Comb> = COMBS
            .iter()
            .map(|&n| {
                let n = len(n);
                Comb {
                    buf: vec![0.0; n],
                    i: 0,
                    // RT60: the tail is 60 dB down after `decay` seconds.
                    feedback: 10f32.powf(-3.0 * n as f32 / (decay * rate as f32)),
                    damp,
                    store: 0.0,
                }
            })
            .collect();
        // The four combs' noise gain, so that the wet level is relative to the dry sound.
        let power: f32 = combs.iter().map(|c| 1.0 / (1.0 - c.feedback.powi(2))).sum();
        Self {
            predelay: vec![0.0; len((preset.late_delay_ms * 44.1) as usize)],
            pre_i: 0,
            combs,
            allpasses: ALLPASSES
                .iter()
                .map(|&n| Allpass {
                    buf: vec![0.0; len(n)],
                    i: 0,
                })
                .collect(),
            input_gain: 1.0 / power.sqrt(),
            wet: 10f32.powf(preset.wet_db / 20.0),
            tail: (decay.min(MAX_TAIL_SECS) * rate as f32) as usize,
        }
    }

    /// Frames the tail rings after the sound ends.
    pub fn tail_frames(&self) -> usize {
        self.tail + self.predelay.len()
    }

    /// The reverb's output for one mono input sample.
    pub fn process(&mut self, x: f32) -> f32 {
        let delayed = std::mem::replace(&mut self.predelay[self.pre_i], x);
        self.pre_i = (self.pre_i + 1) % self.predelay.len();
        let input = delayed * self.input_gain;
        let mut out: f32 = self.combs.iter_mut().map(|c| c.process(input)).sum();
        for a in &mut self.allpasses {
            out = a.process(out);
        }
        out * self.wet
    }
}

/// A decoded sound through its pitch shift and reverb.
pub struct Effects<S: Source> {
    inner: S,
    channels: ChannelCount,
    rate: SampleRate,
    pitch: Option<PitchShift>,
    reverb: Option<Reverb>,
    frame_in: Vec<f32>,
    frame_out: Vec<f32>,
    /// Next sample of `frame_out` to hand out.
    pos: usize,
    ended: bool,
    /// Frames still to play after the sound itself ended.
    tail: usize,
}

impl<S: Source> Effects<S> {
    pub fn new(inner: S, fx: SoundFx) -> Self {
        let channels = inner.channels();
        let rate = inner.sample_rate();
        let ch = usize::from(channels.get());
        let pitch = ((fx.pitch - 1.0).abs() > 1e-3 && fx.pitch > 0.0)
            .then(|| PitchShift::new(fx.pitch, ch, rate.get()));
        let reverb = preset(fx.environment).map(|p| Reverb::new(p, rate.get()));
        let tail = pitch.as_ref().map_or(0, PitchShift::tail_frames)
            + reverb.as_ref().map_or(0, Reverb::tail_frames);
        Self {
            inner,
            channels,
            rate,
            pitch,
            reverb,
            frame_in: vec![0.0; ch],
            frame_out: vec![0.0; ch],
            pos: ch,
            ended: false,
            tail,
        }
    }

    fn passthrough(&self) -> bool {
        self.pitch.is_none() && self.reverb.is_none()
    }

    /// Fills `frame_out` with the next frame; `false` once the sound and its tail are over.
    fn next_frame(&mut self) -> bool {
        if !self.ended {
            for c in 0..self.frame_in.len() {
                match self.inner.next() {
                    Some(s) => self.frame_in[c] = s,
                    None => {
                        self.ended = true;
                        self.frame_in[c..].fill(0.0);
                        break;
                    }
                }
            }
        }
        if self.ended {
            if self.tail == 0 {
                return false;
            }
            self.tail -= 1;
        }
        match &mut self.pitch {
            Some(pitch) => {
                self.frame_out.fill(0.0);
                pitch.process(&self.frame_in, &mut self.frame_out);
            }
            None => self.frame_out.copy_from_slice(&self.frame_in),
        }
        if let Some(reverb) = &mut self.reverb {
            let mono = self.frame_in.iter().sum::<f32>() / self.frame_in.len() as f32;
            let wet = reverb.process(mono);
            for s in &mut self.frame_out {
                *s += wet;
            }
        }
        if self.ended {
            self.frame_in.fill(0.0);
        }
        true
    }
}

impl<S: Source> Iterator for Effects<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if self.passthrough() {
            return self.inner.next();
        }
        if self.pos >= self.frame_out.len() {
            if !self.next_frame() {
                return None;
            }
            self.pos = 0;
        }
        let s = self.frame_out[self.pos];
        self.pos += 1;
        Some(s)
    }
}

impl<S: Source> Source for Effects<S> {
    fn current_span_len(&self) -> Option<usize> {
        if self.passthrough() {
            self.inner.current_span_len()
        } else {
            None
        }
    }

    fn channels(&self) -> ChannelCount {
        if self.passthrough() {
            self.inner.channels()
        } else {
            self.channels
        }
    }

    fn sample_rate(&self) -> SampleRate {
        if self.passthrough() {
            self.inner.sample_rate()
        } else {
            self.rate
        }
    }

    fn total_duration(&self) -> Option<Duration> {
        if self.passthrough() {
            self.inner.total_duration()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dominant period of `samples` (frames between upward zero crossings, averaged).
    fn period(samples: &[f32]) -> f32 {
        let crossings: Vec<usize> = samples
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w[0] <= 0.0 && w[1] > 0.0)
            .map(|(i, _)| i)
            .collect();
        (crossings[crossings.len() - 1] - crossings[0]) as f32 / (crossings.len() - 1) as f32
    }

    #[test]
    fn pitch_moves_the_pitch_not_the_length() {
        let rate = 8000;
        let input: Vec<f32> = (0..rate)
            .map(|i| (2.0 * PI * 200.0 * i as f32 / rate as f32).sin())
            .collect();
        let mut shift = PitchShift::new(0.5, 1, rate);
        let mut out = Vec::with_capacity(input.len());
        for &x in &input {
            let mut o = [0.0];
            shift.process(&[x], &mut o);
            out.push(o[0]);
        }
        // Same length; a 200 Hz tone (40 frames a period) comes out near 100 Hz (80 frames).
        assert_eq!(out.len(), input.len());
        let p = period(&out[2000..]);
        assert!((p - 80.0).abs() < 8.0, "{p}");
        assert!((period(&input) - 40.0).abs() < 0.5);
    }

    #[test]
    fn environments_are_fmod_presets() {
        assert_eq!(preset(-1), None);
        assert_eq!(preset(26), None);
        // `SOUND_AREA_STANDARD_STATION` (parking lot) and `SOUND_AREA_SPACE` (underwater).
        assert_eq!(preset(20).unwrap().decay_secs, 1.7);
        let space = preset(22).unwrap();
        assert_eq!((space.high_cut_hz, space.wet_db), (500.0, 7.0));
    }

    #[test]
    fn a_reverb_rings_and_dies_away() {
        let rate = 8000;
        let mut reverb = Reverb::new(preset(2).unwrap(), rate);
        // An impulse: silence through the late delay, then a tail that fades out.
        let out: Vec<f32> = (0..rate)
            .map(|i| reverb.process(if i == 0 { 1.0 } else { 0.0 }))
            .collect();
        let energy = |s: &[f32]| s.iter().map(|x| x * x).sum::<f32>();
        assert_eq!(out[0], 0.0);
        let early = energy(&out[..2000]);
        let late = energy(&out[6000..]);
        assert!(early > 0.0);
        // A room's 0.4 s decay: a second later it is 60 dB+ down.
        assert!(late < early * 1e-4, "{early} {late}");
    }
}
