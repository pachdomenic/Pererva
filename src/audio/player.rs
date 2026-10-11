//! Програвач звуків в окремому потоці.
//!
//! Аудіопотік ОС відкривається ліниво при першому відтворенні, тож
//! застосунок запускається навіть без звукового пристрою.

use super::synth::Synth;
use crate::config::SoundConfig;
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink, Source};
use std::fs::File;
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Як часто аудіопотік оновлює стан відтворення.
const STATUS_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    Bell,
    Siren,
    AllClear,
    Silence,
    Anthem,
    YellowSiren,
    YellowAllClear,
}

impl Sound {
    fn priority(self) -> u8 {
        match self {
            Sound::Bell => 0,
            Sound::AllClear | Sound::YellowAllClear | Sound::Silence | Sound::Anthem => 1,
            Sound::Siren | Sound::YellowSiren => 2,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Sound::Bell => "Шкільний дзвінок",
            Sound::Siren => "Сирена тривоги",
            Sound::AllClear => "Відбій тривоги",
            Sound::Silence => "Хвилина мовчання",
            Sound::Anthem => "Гімн України",
            Sound::YellowSiren => "Сигнал тривоги жовтого рівня",
            Sound::YellowAllClear => "Відбій тривоги жовтого рівня",
        }
    }

    /// Частина церемонії: хвилина мовчання або гімн.
    pub fn is_memorial(self) -> bool {
        matches!(self, Sound::Silence | Sound::Anthem)
    }
}

enum Command {
    /// Звуки, що звучать один за одним.
    Play(Vec<Sound>, Box<SoundConfig>),
    Stop,
}

pub struct Player {
    tx: Sender<Command>,
    /// Звуки, що звучать зараз (по одному з кожної черги).
    now_playing: Arc<Mutex<Vec<Sound>>>,
}

impl Player {
    /// Запускає потік програвача; `on_error` отримує текст помилок відтворення.
    pub fn spawn(on_error: impl Fn(String) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Command>();
        let now_playing = Arc::new(Mutex::new(Vec::new()));
        let status = Arc::clone(&now_playing);
        thread::Builder::new()
            .name("audio".into())
            .spawn(move || {
                let mut engine = Engine::default();
                loop {
                    let result = match rx.recv_timeout(STATUS_INTERVAL) {
                        Ok(Command::Play(sounds, config)) => engine.play(&sounds, &config),
                        Ok(Command::Stop) => {
                            engine.stop_all();
                            Ok(())
                        }
                        Err(RecvTimeoutError::Timeout) => Ok(()),
                        Err(RecvTimeoutError::Disconnected) => break,
                    };
                    if let Err(message) = result {
                        on_error(message);
                    }
                    if let Ok(mut current) = status.lock() {
                        *current = engine.current();
                    }
                }
            })
            .expect("не вдалося запустити аудіопотік");
        Self { tx, now_playing }
    }

    /// Звук, що звучить зараз (найважливіший, якщо їх кілька).
    pub fn now_playing(&self) -> Option<Sound> {
        let current = self.now_playing.lock().ok()?;
        current.iter().copied().max_by_key(|s| s.priority())
    }

    /// Чи триває хвилина мовчання або гімн.
    pub fn memorial_playing(&self) -> bool {
        self.now_playing
            .lock()
            .is_ok_and(|current| current.iter().any(|s| s.is_memorial()))
    }

    pub fn play(&self, sound: Sound, config: &SoundConfig) {
        self.play_sequence(vec![sound], config);
    }

    /// Подає звуки один за одним без пауз (хвилина мовчання, далі гімн).
    pub fn play_sequence(&self, sounds: Vec<Sound>, config: &SoundConfig) {
        let _ = self
            .tx
            .send(Command::Play(sounds, Box::new(config.clone())));
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Command::Stop);
    }
}

#[derive(Default)]
struct Engine {
    stream: Option<OutputStream>,
    /// Черги звуків: які звуки поставлено в чергу і програвач, що їх подає.
    playing: Vec<(Vec<Sound>, Sink)>,
}

type BoxedSource = Box<dyn Source + Send>;

impl Engine {
    fn play(&mut self, sounds: &[Sound], config: &SoundConfig) -> Result<(), String> {
        let Some(&first) = sounds.first() else {
            return Ok(());
        };
        self.playing.retain(|(_, sink)| !sink.empty());
        match first {
            // Сирена і відбій перекривають усе, що звучить.
            Sound::Siren | Sound::AllClear | Sound::YellowSiren | Sound::YellowAllClear => {
                self.stop_all()
            }
            Sound::Bell => self.stop(Sound::Bell),
            // Церемонія глушить дзвінки, але не сирену і не відбій.
            Sound::Silence | Sound::Anthem => {
                self.stop(Sound::Bell);
                self.stop(Sound::Silence);
                self.stop(Sound::Anthem);
            }
        }

        let mut warnings = Vec::new();
        let mut queued = Vec::new();
        let mut sources = Vec::new();
        for &sound in sounds {
            match source_for(sound, config) {
                Ok((source, warning)) => {
                    warnings.extend(warning);
                    queued.push(sound);
                    sources.push(source);
                }
                Err(e) => warnings.push(e),
            }
        }
        if sources.is_empty() {
            return Err(warnings.join("; "));
        }

        let stream = match &self.stream {
            Some(stream) => stream,
            None => {
                let mut stream = OutputStreamBuilder::open_default_stream()
                    .map_err(|e| format!("звуковий пристрій недоступний: {e}"))?;
                stream.log_on_drop(false);
                self.stream.insert(stream)
            }
        };
        let sink = Sink::connect_new(stream.mixer());
        sink.set_volume(config.volume.clamp(0.0, 1.0));
        for source in sources {
            sink.append(source);
        }
        self.playing.push((queued, sink));
        if warnings.is_empty() {
            Ok(())
        } else {
            Err(warnings.join("; "))
        }
    }

    fn current(&mut self) -> Vec<Sound> {
        self.playing.retain(|(_, sink)| !sink.empty());
        self.playing
            .iter()
            .filter_map(|(sounds, sink)| now_in_queue(sounds, sink.len()))
            .collect()
    }

    fn stop(&mut self, sound: Sound) {
        self.playing.retain(|(sounds, sink)| {
            let hit = sounds.contains(&sound);
            if hit {
                sink.stop();
            }
            !hit
        });
    }

    fn stop_all(&mut self) {
        for (_, sink) in self.playing.drain(..) {
            sink.stop();
        }
    }
}

/// Який звук черги звучить, якщо в програвачі лишилося `left` звуків.
fn now_in_queue(sounds: &[Sound], left: usize) -> Option<Sound> {
    sounds
        .len()
        .checked_sub(left)
        .and_then(|i| sounds.get(i))
        .copied()
}

/// Джерело звуку і, можливо, попередження (файл не відкрився — звучить вбудований).
fn source_for(sound: Sound, config: &SoundConfig) -> Result<(BoxedSource, Option<String>), String> {
    let Some(path) = custom_file(sound, config) else {
        return synthesized(sound, config)
            .map(|source| (source, None))
            .ok_or_else(|| format!("{}: не вибрано файл", sound.title()));
    };
    match load_file(path, sound, config) {
        Ok(source) => Ok((source, None)),
        Err(e) => match synthesized(sound, config) {
            Some(source) => Ok((
                source,
                Some(format!("{}: {e}; звучить вбудований звук", path.display())),
            )),
            None => Err(format!("{}: {e}", path.display())),
        },
    }
}

fn custom_file(sound: Sound, config: &SoundConfig) -> Option<&Path> {
    match sound {
        Sound::Bell => config.bell_file.as_deref(),
        Sound::Siren => config.siren_file.as_deref(),
        Sound::AllClear => config.all_clear_file.as_deref(),
        Sound::Silence => config.silence_file.as_deref(),
        Sound::Anthem => config.anthem_file.as_deref(),
        Sound::YellowSiren => config.yellow_siren_file.as_deref(),
        Sound::YellowAllClear => config.yellow_all_clear_file.as_deref(),
    }
}

/// Вбудований звук; для гімну його немає — потрібен файл.
fn synthesized(sound: Sound, config: &SoundConfig) -> Option<BoxedSource> {
    Some(match sound {
        Sound::Bell => Box::new(Synth::bell(config.bell_seconds)),
        Sound::Siren => Box::new(Synth::siren(config.siren_seconds)),
        Sound::AllClear => Box::new(Synth::all_clear()),
        Sound::Silence => Box::new(Synth::silence()),
        Sound::YellowSiren => Box::new(Synth::warning(config.siren_seconds)),
        // Відбій тривоги жовтого рівня без власного файлу — той самий передзвін.
        Sound::YellowAllClear => Box::new(Synth::all_clear()),
        Sound::Anthem => return None,
    })
}

/// Завантажує власний файл. Дзвінок і сирена повторюються до заданої тривалості,
/// решта звучить повністю.
fn load_file(path: &Path, sound: Sound, config: &SoundConfig) -> Result<BoxedSource, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let decoder = Decoder::try_from(file).map_err(|e| e.to_string())?;
    let seconds = match sound {
        Sound::Bell => config.bell_seconds,
        Sound::Siren | Sound::YellowSiren => config.siren_seconds,
        Sound::AllClear | Sound::YellowAllClear | Sound::Silence | Sound::Anthem => {
            return Ok(Box::new(decoder));
        }
    };
    Ok(Box::new(
        decoder
            .buffered()
            .repeat_infinite()
            .take_duration(Duration::from_secs(u64::from(seconds))),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_reports_error() {
        let config = SoundConfig::default();
        let result = load_file(Path::new("/немає/такого/файлу.wav"), Sound::Bell, &config);
        assert!(result.is_err());
    }

    #[test]
    fn custom_files_are_picked_per_sound() {
        let config = SoundConfig {
            siren_file: Some("siren.mp3".into()),
            ..SoundConfig::default()
        };
        assert_eq!(custom_file(Sound::Bell, &config), None);
        assert_eq!(
            custom_file(Sound::Siren, &config),
            Some(Path::new("siren.mp3"))
        );
    }

    #[test]
    fn synthesized_durations_follow_config() {
        let config = SoundConfig {
            bell_seconds: 3,
            siren_seconds: 40,
            ..SoundConfig::default()
        };
        let duration = |sound| synthesized(sound, &config).unwrap().total_duration();
        assert_eq!(duration(Sound::Bell), Some(Duration::from_secs(3)));
        assert_eq!(duration(Sound::Siren), Some(Duration::from_secs(40)));
        assert_eq!(duration(Sound::Silence), Some(Duration::from_secs(63)));
    }

    #[test]
    fn anthem_needs_a_file() {
        let config = SoundConfig::default();
        assert!(synthesized(Sound::Anthem, &config).is_none());
        let error = source_for(Sound::Anthem, &config).err().unwrap();
        assert!(error.contains("не вибрано файл"));

        let config = SoundConfig {
            anthem_file: Some("/немає/гімну.mp3".into()),
            silence_file: Some("/немає/тиші.mp3".into()),
            ..SoundConfig::default()
        };
        assert!(source_for(Sound::Anthem, &config).is_err());
        // Хвилина мовчання повертається до вбудованого метронома.
        let (_, warning) = source_for(Sound::Silence, &config).unwrap();
        assert!(warning.unwrap().contains("вбудований"));
    }

    #[test]
    fn queue_position_follows_sink_length() {
        let queue = [Sound::Silence, Sound::Anthem];
        assert_eq!(now_in_queue(&queue, 2), Some(Sound::Silence));
        assert_eq!(now_in_queue(&queue, 1), Some(Sound::Anthem));
        assert_eq!(now_in_queue(&queue, 0), None);
        assert_eq!(now_in_queue(&queue, 3), None);
    }
}
