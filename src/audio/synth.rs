//! Синтез звуків без зовнішніх файлів.
//!
//! - шкільний дзвінок — електромеханічний дзвоник: молоточок б'є по чаші
//!   ~18 разів на секунду, чаша звучить набором негармонічних обертонів;
//! - сирена повітряної тривоги — механічна сирена, що розкручується,
//!   завиває (плавно піднімає і опускає тон) і зупиняється;
//! - відбій — спокійний висхідний триголосий передзвін.

use rodio::Source;
use rodio::source::SeekError;
use std::f64::consts::TAU;
use std::time::Duration;

pub const SAMPLE_RATE: u32 = 44_100;

/// Тривалість сигналу відбою.
pub const ALL_CLEAR_SECONDS: f64 = 4.8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynthKind {
    Bell,
    Siren,
    AllClear,
}

/// Нескінченне джерело звуку обмеженої тривалості.
#[derive(Debug, Clone)]
pub struct Synth {
    kind: SynthKind,
    n: u64,
    total: u64,
    phase: f64,
}

impl Synth {
    pub fn new(kind: SynthKind, duration: Duration) -> Self {
        let total = (duration.as_secs_f64() * f64::from(SAMPLE_RATE)).round() as u64;
        Self {
            kind,
            n: 0,
            total,
            phase: 0.0,
        }
    }

    pub fn bell(seconds: u32) -> Self {
        Self::new(SynthKind::Bell, Duration::from_secs(u64::from(seconds)))
    }

    pub fn siren(seconds: u32) -> Self {
        Self::new(SynthKind::Siren, Duration::from_secs(u64::from(seconds)))
    }

    pub fn all_clear() -> Self {
        Self::new(
            SynthKind::AllClear,
            Duration::from_secs_f64(ALL_CLEAR_SECONDS),
        )
    }

    fn duration_secs(&self) -> f64 {
        self.total as f64 / f64::from(SAMPLE_RATE)
    }
}

impl Iterator for Synth {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.n >= self.total {
            return None;
        }
        let t = self.n as f64 / f64::from(SAMPLE_RATE);
        let length = self.duration_secs();
        let sample = match self.kind {
            SynthKind::Bell => bell(t, length),
            SynthKind::AllClear => all_clear(t),
            SynthKind::Siren => {
                let freq = siren_frequency(t, length);
                self.phase = (self.phase + TAU * freq / f64::from(SAMPLE_RATE)) % TAU;
                siren_wave(self.phase) * siren_level(t, length)
            }
        };
        self.n += 1;
        Some(sample.clamp(-1.0, 1.0) as f32)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = (self.total - self.n) as usize;
        (left, Some(left))
    }
}

impl Source for Synth {
    fn current_span_len(&self) -> Option<usize> {
        Some((self.total - self.n) as usize)
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(self.duration_secs()))
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.n = ((pos.as_secs_f64() * f64::from(SAMPLE_RATE)) as u64).min(self.total);
        Ok(())
    }
}

/// Плавний вхід і вихід, щоб не було клацань.
fn edges(t: f64, length: f64, fade: f64) -> f64 {
    (t / fade).min(1.0).min(((length - t) / fade).max(0.0))
}

/// Обертони металевої чаші дзвоника: (множник частоти, амплітуда).
const BELL_PARTIALS: [(f64, f64); 5] = [
    (1.0, 1.0),
    (2.32, 0.55),
    (4.25, 0.35),
    (6.63, 0.22),
    (9.38, 0.12),
];
const BELL_FUNDAMENTAL: f64 = 1_150.0;
const BELL_STRIKES_PER_SECOND: f64 = 18.0;

fn bell(t: f64, length: f64) -> f64 {
    let since_strike = (t * BELL_STRIKES_PER_SECOND).fract() / BELL_STRIKES_PER_SECOND;
    // Удар молоточка — різкий сплеск, далі чаша продовжує дзвеніти.
    let strike = (-since_strike * 60.0).exp();
    let level = 0.55 + 0.45 * strike;
    let tone: f64 = BELL_PARTIALS
        .iter()
        .map(|&(ratio, amp)| amp * (TAU * BELL_FUNDAMENTAL * ratio * t).sin())
        .sum::<f64>()
        / 2.24;
    // Короткий шумоподібний «клац» молоточка.
    let click = (TAU * 3_700.0 * t).sin() * (-since_strike * 400.0).exp() * 0.25;
    (tone * level + click) * edges(t, length, 0.03) * 0.9
}

const SIREN_LOW: f64 = 260.0;
const SIREN_HIGH: f64 = 640.0;
const SIREN_SPIN_UP: f64 = 4.0;
const SIREN_SPIN_DOWN: f64 = 5.0;
/// Період завивання: підйом, утримання, спад.
const SIREN_CYCLE: f64 = 10.0;

/// Частота тону сирени у момент `t`.
pub fn siren_frequency(t: f64, length: f64) -> f64 {
    let spin_down_start = (length - SIREN_SPIN_DOWN).max(SIREN_SPIN_UP);
    if t < SIREN_SPIN_UP {
        // Ротор розкручується від низького гулу до верхнього тону.
        let x = t / SIREN_SPIN_UP;
        return 60.0 + (SIREN_HIGH - 60.0) * (1.0 - (1.0 - x).powi(2));
    }
    if t >= spin_down_start {
        let x = ((t - spin_down_start) / SIREN_SPIN_DOWN).min(1.0);
        let from = wail(spin_down_start - SIREN_SPIN_UP);
        return from + (40.0 - from) * (1.0 - (1.0 - x).powi(2));
    }
    wail(t - SIREN_SPIN_UP)
}

/// Завивання: утримання вгорі, плавний спад і новий підйом.
fn wail(t: f64) -> f64 {
    let x = (t % SIREN_CYCLE) / SIREN_CYCLE;
    let shape = if x < 0.2 {
        1.0
    } else {
        0.5 + 0.5 * (TAU * (x - 0.2) / 0.8).cos()
    };
    SIREN_LOW + (SIREN_HIGH - SIREN_LOW) * shape
}

/// Тембр механічної сирени: м'яко обмежена синусоїда з додатковими гармоніками.
fn siren_wave(phase: f64) -> f64 {
    let base = (2.4 * phase.sin()).tanh();
    let overtone = 0.18 * (2.0 * phase).sin() + 0.08 * (3.0 * phase).sin();
    (base + overtone) / 1.26
}

fn siren_level(t: f64, length: f64) -> f64 {
    let up = (t / 1.5).min(1.0);
    let down = ((length - t) / SIREN_SPIN_DOWN).clamp(0.0, 1.0);
    0.85 * up * down.sqrt()
}

/// Ноти передзвону відбою: (початок, частота).
const ALL_CLEAR_NOTES: [(f64, f64); 6] = [
    (0.0, 523.25),
    (0.45, 659.25),
    (0.9, 783.99),
    (2.2, 523.25),
    (2.65, 659.25),
    (3.1, 1_046.5),
];

fn all_clear(t: f64) -> f64 {
    let sum: f64 = ALL_CLEAR_NOTES
        .iter()
        .filter(|(start, _)| t >= *start)
        .map(|&(start, freq)| {
            let dt = t - start;
            let env = (dt / 0.01).min(1.0) * (-dt * 2.2).exp();
            env * ((TAU * freq * dt).sin() + 0.3 * (TAU * 2.0 * freq * dt).sin())
        })
        .sum();
    sum * 0.45 * edges(t, ALL_CLEAR_SECONDS, 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(synth: Synth) -> Vec<f32> {
        synth.collect()
    }

    fn rms(samples: &[f32]) -> f64 {
        (samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
    }

    #[test]
    fn lengths_match_duration() {
        assert_eq!(render(Synth::bell(2)).len(), 2 * SAMPLE_RATE as usize);
        assert_eq!(render(Synth::siren(12)).len(), 12 * SAMPLE_RATE as usize);
        let clear = render(Synth::all_clear()).len() as f64 / f64::from(SAMPLE_RATE);
        assert!((clear - ALL_CLEAR_SECONDS).abs() < 1e-3);
    }

    #[test]
    fn samples_are_in_range_and_audible() {
        for synth in [Synth::bell(3), Synth::siren(15), Synth::all_clear()] {
            let samples = render(synth);
            assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
            assert!(rms(&samples) > 0.05, "звук занадто тихий");
        }
    }

    #[test]
    fn starts_and_ends_silently() {
        for synth in [Synth::bell(3), Synth::siren(15), Synth::all_clear()] {
            let samples = render(synth);
            assert!(samples[0].abs() < 1e-3);
            assert!(samples.last().unwrap().abs() < 1e-2);
        }
    }

    #[test]
    fn siren_wails_between_low_and_high() {
        let length = 60.0;
        assert!(siren_frequency(0.0, length) < 100.0);
        let steady: Vec<f64> = (40..500)
            .map(|i| siren_frequency(f64::from(i) / 10.0, length))
            .collect();
        let max = steady.iter().cloned().fold(f64::MIN, f64::max);
        let min = steady.iter().cloned().fold(f64::MAX, f64::min);
        assert!((max - SIREN_HIGH).abs() < 1.0);
        assert!((min - SIREN_LOW).abs() < 5.0);
        assert!(siren_frequency(length, length) < 50.0);
    }

    #[test]
    fn frequency_is_continuous() {
        let length = 30.0;
        let step = 1.0 / f64::from(SAMPLE_RATE);
        let mut prev = siren_frequency(0.0, length);
        let mut t = step;
        while t < length {
            let f = siren_frequency(t, length);
            assert!((f - prev).abs() < 1.0, "стрибок частоти на {t:.3} с");
            prev = f;
            t += step * 50.0;
        }
    }
}
