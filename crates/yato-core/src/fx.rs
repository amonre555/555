//! Effets et analyse du son.
//!
//! Chaque morceau passe dans [`FxSource`] avant d'arriver au lecteur :
//! - un égaliseur 3 bandes (basses, médiums, aigus), en filtres biquad classiques (RBJ) ;
//! - une réverbération légère (4 filtres en peigne et 2 passe-tout, à la Freeverb) ;
//! - une mesure du niveau des basses, médiums et aigus, lue par l'interface pour le visualiseur.
//!
//! Les réglages vivent dans [`Fx`], partagé entre les deux lecteurs : un changement s'entend
//! tout de suite, sans recharger le morceau. La vitesse (« slowed », « sped up ») est réglée à
//! part, sur les lecteurs eux-mêmes.

use std::{
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
    time::Duration,
};

use rodio::{ChannelCount, Sample, SampleRate, Source, source::SeekError};
use serde::{Deserialize, Serialize};

/// Réglages des effets, gardés avec les réglages de l'app.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FxSettings {
    /// Gain des basses, en dB (-12 à +12).
    pub bass: f32,
    /// Gain des médiums, en dB.
    pub mid: f32,
    /// Gain des aigus, en dB.
    pub treble: f32,
    /// Quantité de réverbération (0 à 1).
    pub reverb: f32,
    /// Vitesse de lecture (0,5 à 1,5) ; le son monte ou descend avec, comme un vinyle.
    pub speed: f32,
}

impl Default for FxSettings {
    fn default() -> Self {
        Self { bass: 0.0, mid: 0.0, treble: 0.0, reverb: 0.0, speed: 1.0 }
    }
}

impl FxSettings {
    /// Les valeurs ramenées dans leurs bornes.
    pub fn clamped(self) -> Self {
        let db = |v: f32| if v.is_finite() { v.clamp(-12.0, 12.0) } else { 0.0 };
        Self {
            bass: db(self.bass),
            mid: db(self.mid),
            treble: db(self.treble),
            reverb: if self.reverb.is_finite() { self.reverb.clamp(0.0, 1.0) } else { 0.0 },
            speed: if self.speed.is_finite() { self.speed.clamp(0.5, 1.5) } else { 1.0 },
        }
    }
}

/// Un `f32` qu'on peut lire et écrire depuis plusieurs fils.
#[derive(Default)]
struct AtomicF32(AtomicU32);

impl AtomicF32 {
    fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }
}

/// Réglages en cours et dernière mesure du son, partagés entre les lecteurs et l'interface.
#[derive(Default)]
pub struct Fx {
    bass: AtomicF32,
    mid: AtomicF32,
    treble: AtomicF32,
    reverb: AtomicF32,
    /// Incrémenté à chaque changement : les sources recalculent leurs filtres.
    version: AtomicU64,
    /// Basses, médiums, aigus, ensemble : de 0 à 1 environ.
    levels: [AtomicF32; 4],
}

impl Fx {
    pub fn set(&self, s: FxSettings) {
        self.bass.set(s.bass);
        self.mid.set(s.mid);
        self.treble.set(s.treble);
        self.reverb.set(s.reverb);
        self.version.fetch_add(1, Ordering::Release);
    }

    /// Dernière mesure : basses, médiums, aigus, ensemble.
    pub fn levels(&self) -> [f32; 4] {
        [0, 1, 2, 3].map(|i| self.levels[i].get())
    }
}

/// Filtre biquad (forme directe transposée II), coefficients déjà divisés par a0.
#[derive(Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Biquad {
    const FLAT: Self = Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    fn new(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }

    /// Plateau grave (pente 1) : `db` sous `freq`.
    fn low_shelf(freq: f32, db: f32, rate: f32) -> Self {
        if db.abs() < 0.01 {
            return Self::FLAT;
        }
        let a = 10f32.powf(db / 40.0);
        let (sin, cos) = (std::f32::consts::TAU * freq / rate).sin_cos();
        let k = 2.0 * a.sqrt() * sin / 2.0 * std::f32::consts::SQRT_2;
        Self::new(
            a * ((a + 1.0) - (a - 1.0) * cos + k),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
            a * ((a + 1.0) - (a - 1.0) * cos - k),
            (a + 1.0) + (a - 1.0) * cos + k,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos),
            (a + 1.0) + (a - 1.0) * cos - k,
        )
    }

    /// Plateau aigu (pente 1) : `db` au-dessus de `freq`.
    fn high_shelf(freq: f32, db: f32, rate: f32) -> Self {
        if db.abs() < 0.01 {
            return Self::FLAT;
        }
        let a = 10f32.powf(db / 40.0);
        let (sin, cos) = (std::f32::consts::TAU * freq / rate).sin_cos();
        let k = 2.0 * a.sqrt() * sin / 2.0 * std::f32::consts::SQRT_2;
        Self::new(
            a * ((a + 1.0) + (a - 1.0) * cos + k),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
            a * ((a + 1.0) + (a - 1.0) * cos - k),
            (a + 1.0) - (a - 1.0) * cos + k,
            2.0 * ((a - 1.0) - (a + 1.0) * cos),
            (a + 1.0) - (a - 1.0) * cos - k,
        )
    }

    /// Cloche autour de `freq`.
    fn peaking(freq: f32, db: f32, q: f32, rate: f32) -> Self {
        if db.abs() < 0.01 {
            return Self::FLAT;
        }
        let a = 10f32.powf(db / 40.0);
        let (sin, cos) = (std::f32::consts::TAU * freq / rate).sin_cos();
        let alpha = sin / (2.0 * q);
        Self::new(1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a)
    }

    fn run(&self, x: f32, z: &mut [f32; 2]) -> f32 {
        let y = self.b0 * x + z[0];
        z[0] = self.b1 * x - self.a1 * y + z[1];
        z[1] = self.b2 * x - self.a2 * y;
        y
    }
}

/// Ligne à retard circulaire.
struct Delay {
    buf: Vec<f32>,
    pos: usize,
}

impl Delay {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(1)], pos: 0 }
    }
    fn read(&self) -> f32 {
        self.buf[self.pos]
    }
    fn write_advance(&mut self, v: f32) {
        self.buf[self.pos] = v;
        self.pos = (self.pos + 1) % self.buf.len();
    }
}

/// Réverbération mono, à la Freeverb : 4 peignes en parallèle, puis 2 passe-tout.
struct Reverb {
    combs: Vec<(Delay, f32)>,
    allpasses: Vec<Delay>,
}

impl Reverb {
    fn new(rate: f32) -> Self {
        let len = |ms: f32| (ms * rate / 1000.0) as usize;
        Self {
            combs: [29.7, 37.1, 41.1, 43.7].iter().map(|&ms| (Delay::new(len(ms)), 0.0)).collect(),
            allpasses: [5.0, 1.7].iter().map(|&ms| Delay::new(len(ms))).collect(),
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        const FEEDBACK: f32 = 0.84;
        const DAMP: f32 = 0.25;
        let input = x * 0.03;
        let mut out = 0.0;
        for (d, filtered) in &mut self.combs {
            let y = d.read();
            *filtered = y * (1.0 - DAMP) + *filtered * DAMP;
            d.write_advance(input + *filtered * FEEDBACK);
            out += y;
        }
        for d in &mut self.allpasses {
            let b = d.read();
            d.write_advance(out + b * 0.5);
            out = b - out;
        }
        out * 3.0
    }
}

/// Mesure des niveaux, sur le premier canal : deux passe-bas d'un pôle séparent les basses
/// (sous 150 Hz), les médiums et les aigus (au-dessus de 4 kHz).
struct Meter {
    lp_low: f32,
    lp_high: f32,
    k_low: f32,
    k_high: f32,
    sums: [f32; 4],
    count: usize,
}

impl Meter {
    /// Une mesure toutes les 1024 trames (environ 20 ms).
    const WINDOW: usize = 1024;

    fn new(rate: f32) -> Self {
        let k = |f: f32| 1.0 - (-std::f32::consts::TAU * f / rate).exp();
        Self { lp_low: 0.0, lp_high: 0.0, k_low: k(150.0), k_high: k(4000.0), sums: [0.0; 4], count: 0 }
    }

    fn feed(&mut self, x: f32, fx: &Fx) {
        self.lp_low += self.k_low * (x - self.lp_low);
        self.lp_high += self.k_high * (x - self.lp_high);
        let bands = [self.lp_low, self.lp_high - self.lp_low, x - self.lp_high, x];
        for (s, b) in self.sums.iter_mut().zip(bands) {
            *s += b * b;
        }
        self.count += 1;
        if self.count == Self::WINDOW {
            // Gains choisis à l'oreille (à l'œil) : une musique forte approche 1.
            const GAIN: [f32; 4] = [3.0, 4.5, 9.0, 3.0];
            for i in 0..4 {
                let rms = (self.sums[i] / Self::WINDOW as f32).sqrt();
                fx.levels[i].set((rms * GAIN[i]).min(1.0));
            }
            self.sums = [0.0; 4];
            self.count = 0;
        }
    }
}

/// Position dans le morceau, en secondes (bits d'un `f64`), écrite par [`FxSource`].
///
/// Elle est comptée en temps du morceau, avant le changement de vitesse : la position que
/// donne rodio, elle, compte le temps écoulé, et se trompe dès que la vitesse change.
#[derive(Default)]
pub struct Clock(AtomicU64);

impl Clock {
    pub fn get(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }
    pub fn set(&self, seconds: f64) {
        self.0.store(seconds.to_bits(), Ordering::Relaxed);
    }
}

/// Source qui applique l'égaliseur et la réverbération, et mesure le son au passage.
pub struct FxSource<S> {
    inner: S,
    fx: std::sync::Arc<Fx>,
    clock: std::sync::Arc<Clock>,
    /// Position du dernier saut (ou du dernier changement de format), et trames lues depuis.
    base: f64,
    frames: u64,
    channels: usize,
    rate: u32,
    /// Canal du prochain échantillon (dans la trame en cours).
    ch: usize,
    version: u64,
    eq: [Biquad; 3],
    /// État des 3 filtres, par canal.
    eq_state: Vec<[[f32; 2]; 3]>,
    eq_on: bool,
    reverb: Reverb,
    wet: f32,
    /// Somme des canaux de la trame en cours, et réverbération calculée sur la précédente.
    frame_sum: f32,
    frame_wet: f32,
    meter: Meter,
}

impl<S: Source> FxSource<S> {
    /// `clock` repart de zéro : la position compte depuis l'ouverture du décodeur.
    pub fn new(inner: S, fx: std::sync::Arc<Fx>, clock: std::sync::Arc<Clock>) -> Self {
        clock.set(0.0);
        let channels = usize::from(inner.channels().get());
        let rate = inner.sample_rate().get();
        let mut s = Self {
            inner,
            fx,
            clock,
            base: 0.0,
            frames: 0,
            channels,
            rate,
            ch: 0,
            version: u64::MAX,
            eq: [Biquad::FLAT; 3],
            eq_state: vec![[[0.0; 2]; 3]; channels],
            eq_on: false,
            reverb: Reverb::new(rate as f32),
            wet: 0.0,
            frame_sum: 0.0,
            frame_wet: 0.0,
            meter: Meter::new(rate as f32),
        };
        s.refresh();
        s
    }

    /// Relit les réglages s'ils ont changé.
    fn refresh(&mut self) {
        let v = self.fx.version.load(Ordering::Acquire);
        if v == self.version {
            return;
        }
        self.version = v;
        let r = self.rate as f32;
        let (bass, mid, treble) = (self.fx.bass.get(), self.fx.mid.get(), self.fx.treble.get());
        self.eq = [Biquad::low_shelf(120.0, bass, r), Biquad::peaking(1000.0, mid, 0.8, r), Biquad::high_shelf(6000.0, treble, r)];
        self.eq_on = [bass, mid, treble].iter().any(|g| g.abs() >= 0.01);
        self.wet = self.fx.reverb.get();
    }

    /// Le format a changé (nouveau segment du fichier) : on repart de filtres vides.
    fn reformat(&mut self, channels: usize, rate: u32) {
        self.base += self.frames as f64 / f64::from(self.rate);
        self.frames = 0;
        self.channels = channels;
        self.rate = rate;
        self.eq_state = vec![[[0.0; 2]; 3]; channels];
        self.reverb = Reverb::new(rate as f32);
        self.meter = Meter::new(rate as f32);
        self.version = u64::MAX;
        self.refresh();
    }
}

impl<S: Source> Iterator for FxSource<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if self.ch == 0 {
            let (channels, rate) = (usize::from(self.inner.channels().get()), self.inner.sample_rate().get());
            if channels != self.channels || rate != self.rate {
                self.reformat(channels, rate);
            }
            self.refresh();
            // Réverbération de la trame précédente, appliquée à tous les canaux de celle-ci
            // (un échantillon de décalage, inaudible).
            let mono = self.frame_sum / self.channels as f32;
            self.frame_wet = if self.wet > 0.0 { self.reverb.run(mono) * self.wet } else { 0.0 };
            self.frame_sum = 0.0;
        }
        let x = self.inner.next()?;
        if self.ch == 0 {
            self.frames += 1;
            self.clock.set(self.base + self.frames as f64 / f64::from(self.rate));
        }
        let ch = self.ch;
        self.ch = (self.ch + 1) % self.channels;
        if ch == 0 {
            self.meter.feed(x, &self.fx);
        }
        self.frame_sum += x;

        let mut y = x;
        if self.eq_on {
            let state = &mut self.eq_state[ch];
            for (f, z) in self.eq.iter().zip(state.iter_mut()) {
                y = f.run(y, z);
            }
        }
        if self.wet > 0.0 {
            y = y * (1.0 - 0.35 * self.wet) + self.frame_wet;
        }
        Some(if self.eq_on || self.wet > 0.0 { y.clamp(-1.0, 1.0) } else { y })
    }
}

impl<S: Source> Source for FxSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }
    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(pos)?;
        self.ch = 0;
        self.base = pos.as_secs_f64();
        self.frames = 0;
        self.clock.set(self.base);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn sine(freq: f32, rate: u32, secs: f32) -> rodio::buffer::SamplesBuffer {
        let n = (rate as f32 * secs) as usize;
        let data: Vec<f32> = (0..n).map(|i| (std::f32::consts::TAU * freq * i as f32 / rate as f32).sin() * 0.5).collect();
        rodio::buffer::SamplesBuffer::new(ChannelCount::new(1).unwrap(), SampleRate::new(rate).unwrap(), data)
    }

    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt()
    }

    #[test]
    fn flat_settings_leave_the_sound_untouched() {
        let fx = Arc::new(Fx::default());
        let out: Vec<f32> = FxSource::new(sine(440.0, 48_000, 0.1), fx, Default::default()).collect();
        let original: Vec<f32> = sine(440.0, 48_000, 0.1).collect();
        assert_eq!(out, original);
    }

    #[test]
    fn bass_boost_raises_low_notes_but_not_high_ones() {
        let fx = Arc::new(Fx::default());
        fx.set(FxSettings { bass: 9.0, ..Default::default() });
        let low: Vec<f32> = FxSource::new(sine(60.0, 48_000, 0.5), fx.clone(), Default::default()).skip(4800).collect();
        let high: Vec<f32> = FxSource::new(sine(5000.0, 48_000, 0.5), fx, Default::default()).skip(4800).collect();
        assert!(rms(&low) > 0.5 / 2f32.sqrt() * 2.0, "basses : {}", rms(&low));
        assert!((rms(&high) - 0.5 / 2f32.sqrt()).abs() < 0.03, "aigus : {}", rms(&high));
    }

    #[test]
    fn meter_sees_bass_in_a_low_note() {
        let fx = Arc::new(Fx::default());
        let _: Vec<f32> = FxSource::new(sine(60.0, 48_000, 0.2), fx.clone(), Default::default()).collect();
        let [bass, _, treble, all] = fx.levels();
        assert!(bass > 0.5 && treble < 0.1 && all > 0.5, "{:?}", fx.levels());
    }

    #[test]
    fn clock_counts_song_time_and_follows_seeks() {
        let clock = Arc::new(Clock::default());
        let mut src = FxSource::new(sine(440.0, 48_000, 2.0), Arc::new(Fx::default()), clock.clone());
        for _ in 0..24_000 {
            src.next();
        }
        assert!((clock.get() - 0.5).abs() < 1e-3, "{}", clock.get());
        src.try_seek(Duration::from_secs_f64(1.2)).unwrap();
        assert!((clock.get() - 1.2).abs() < 1e-9);
    }

    #[test]
    fn reverb_keeps_ringing_after_the_sound() {
        let fx = Arc::new(Fx::default());
        fx.set(FxSettings { reverb: 1.0, ..Default::default() });
        let mut data: Vec<f32> = sine(440.0, 48_000, 0.1).collect();
        data.extend(std::iter::repeat_n(0.0, 4800));
        let src = rodio::buffer::SamplesBuffer::new(ChannelCount::new(1).unwrap(), SampleRate::new(48_000).unwrap(), data);
        let out: Vec<f32> = FxSource::new(src, fx, Default::default()).collect();
        assert!(rms(&out[out.len() - 2400..]) > 0.005);
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use std::sync::Arc;

    #[test]
    #[ignore]
    fn cost_of_60_seconds_of_stereo() {
        let n = 48_000 * 2 * 60;
        let data: Vec<f32> = (0..n).map(|i| ((i as f32) * 0.01).sin() * 0.4).collect();
        let src = rodio::buffer::SamplesBuffer::new(ChannelCount::new(2).unwrap(), SampleRate::new(48_000).unwrap(), data);
        let fx = Arc::new(Fx::default());
        fx.set(FxSettings { bass: 6.0, mid: -3.0, treble: 4.0, reverb: 0.5, speed: 1.0 });
        let t = std::time::Instant::now();
        let sum: f32 = FxSource::new(src, fx, Default::default()).sum();
        eprintln!("60 s de son traités en {:?} (somme {sum})", t.elapsed());
    }
}
