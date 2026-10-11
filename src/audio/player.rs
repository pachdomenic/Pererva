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
}

impl Sound {
    fn priority(self) -> u8 {
        match self {
            Sound::Bell => 0,
            Sound::AllClear => 1,
            Sound::Siren => 2,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Sound::Bell => "Шкільний дзвінок",
            Sound::Siren => "Сирена тривоги",
            Sound::AllClear => "Відбій тривоги",
        }
    }
}

enum Command {
    Play(Sound, SoundConfig),
    Stop,
}

pub struct Player {
    tx: Sender<Command>,
    now_playing: Arc<Mutex<Option<Sound>>>,
}

impl Player {
    /// Запускає потік програвача; `on_error` отримує текст помилок відтворення.
    pub fn spawn(on_error: impl Fn(String) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Command>();
        let now_playing = Arc::new(Mutex::new(None));
        let status = Arc::clone(&now_playing);
        thread::Builder::new()
            .name("audio".into())
            .spawn(move || {
                let mut engine = Engine::default();
                loop {
                    let result = match rx.recv_timeout(STATUS_INTERVAL) {
                        Ok(Command::Play(sound, config)) => engine.play(sound, &config),
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
        self.now_playing.lock().ok().and_then(|current| *current)
    }

    pub fn play(&self, sound: Sound, config: &SoundConfig) {
        let _ = self.tx.send(Command::Play(sound, config.clone()));
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Command::Stop);
    }
}

#[derive(Default)]
struct Engine {
    stream: Option<OutputStream>,
    playing: Vec<(Sound, Sink)>,
}

type BoxedSource = Box<dyn Source + Send>;

impl Engine {
    fn play(&mut self, sound: Sound, config: &SoundConfig) -> Result<(), String> {
        self.playing.retain(|(_, sink)| !sink.empty());
        match sound {
            // Сирена і відбій перекривають усе, що звучить.
            Sound::Siren | Sound::AllClear => self.stop_all(),
            Sound::Bell => self.stop(Sound::Bell),
        }

        let mut warning = None;
        let source = match custom_file(sound, config) {
            Some(path) => load_file(path, sound, config).unwrap_or_else(|e| {
                warning = Some(format!("{}: {e}; звучить вбудований звук", path.display()));
                synthesized(sound, config)
            }),
            None => synthesized(sound, config),
        };

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
        sink.append(source);
        self.playing.push((sound, sink));
        warning.map_or(Ok(()), Err)
    }

    fn current(&mut self) -> Option<Sound> {
        self.playing.retain(|(_, sink)| !sink.empty());
        self.playing
            .iter()
            .map(|(sound, _)| *sound)
            .max_by_key(|s| s.priority())
    }

    fn stop(&mut self, sound: Sound) {
        self.playing.retain(|(s, sink)| {
            if *s == sound {
                sink.stop();
            }
            *s != sound
        });
    }

    fn stop_all(&mut self) {
        for (_, sink) in self.playing.drain(..) {
            sink.stop();
        }
    }
}

fn custom_file(sound: Sound, config: &SoundConfig) -> Option<&Path> {
    match sound {
        Sound::Bell => config.bell_file.as_deref(),
        Sound::Siren => config.siren_file.as_deref(),
        Sound::AllClear => config.all_clear_file.as_deref(),
    }
}

fn synthesized(sound: Sound, config: &SoundConfig) -> BoxedSource {
    match sound {
        Sound::Bell => Box::new(Synth::bell(config.bell_seconds)),
        Sound::Siren => Box::new(Synth::siren(config.siren_seconds)),
        Sound::AllClear => Box::new(Synth::all_clear()),
    }
}

/// Завантажує власний файл. Дзвінок і сирена повторюються до заданої тривалості.
fn load_file(path: &Path, sound: Sound, config: &SoundConfig) -> Result<BoxedSource, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let decoder = Decoder::try_from(file).map_err(|e| e.to_string())?;
    let seconds = match sound {
        Sound::Bell => config.bell_seconds,
        Sound::Siren => config.siren_seconds,
        Sound::AllClear => return Ok(Box::new(decoder)),
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
        let bell = synthesized(Sound::Bell, &config).total_duration().unwrap();
        let siren = synthesized(Sound::Siren, &config).total_duration().unwrap();
        assert_eq!(bell, Duration::from_secs(3));
        assert_eq!(siren, Duration::from_secs(40));
    }
}
