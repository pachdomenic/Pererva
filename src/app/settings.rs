//! Перетворення між налаштуваннями застосунку і даними форми інтерфейсу.

use crate::SettingsData;
use crate::alert::regions::{self, REGIONS};
use crate::config::{AlertProvider, Config, ThemeMode};
use std::path::PathBuf;

fn path_to_string(path: &Option<PathBuf>) -> slint::SharedString {
    path.as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default()
        .into()
}

fn string_to_path(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

pub fn to_form(config: &Config) -> SettingsData {
    SettingsData {
        alert_enabled: config.alert.enabled,
        provider: config.alert.provider.index() as i32,
        token: config.alert.token.as_str().into(),
        region: regions::index_of(config.alert.region_uid).unwrap_or(0) as i32,
        poll_seconds: config.alert.poll_seconds as i32,
        mute_bells: config.alert.mute_bells_during_alert,
        partial_is_alert: config.alert.partial_is_alert,
        play_all_clear: config.alert.play_all_clear,
        volume: config.sound.volume,
        bell_seconds: config.sound.bell_seconds as i32,
        siren_seconds: config.sound.siren_seconds as i32,
        bell_file: path_to_string(&config.sound.bell_file),
        siren_file: path_to_string(&config.sound.siren_file),
        all_clear_file: path_to_string(&config.sound.all_clear_file),
        theme: config.ui.theme.index(),
    }
}

/// Застосовує дані форми до копії налаштувань (розклад не змінюється).
pub fn apply_form(base: &Config, form: &SettingsData) -> Config {
    let mut config = base.clone();
    let alert = &mut config.alert;
    alert.enabled = form.alert_enabled;
    alert.provider = AlertProvider::from_index(form.provider.max(0) as usize);
    alert.token = form.token.trim().to_owned();
    alert.region_uid = REGIONS
        .get(form.region.max(0) as usize)
        .map_or(alert.region_uid, |r| r.uid);
    alert.poll_seconds = form.poll_seconds.max(0) as u64;
    alert.mute_bells_during_alert = form.mute_bells;
    alert.partial_is_alert = form.partial_is_alert;
    alert.play_all_clear = form.play_all_clear;

    let sound = &mut config.sound;
    sound.volume = form.volume;
    sound.bell_seconds = form.bell_seconds.max(0) as u32;
    sound.siren_seconds = form.siren_seconds.max(0) as u32;
    sound.bell_file = string_to_path(&form.bell_file);
    sound.siren_file = string_to_path(&form.siren_file);
    sound.all_clear_file = string_to_path(&form.all_clear_file);

    config.ui.theme = ThemeMode::from_index(form.theme);
    config.normalize();
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_roundtrip_keeps_config() {
        let mut config = Config::default();
        config.alert.provider = AlertProvider::AlertsInUa;
        config.alert.region_uid = 27;
        config.sound.siren_file = Some("/tmp/siren.mp3".into());
        config.ui.theme = ThemeMode::Dark;
        assert_eq!(apply_form(&config, &to_form(&config)), config);
    }

    #[test]
    fn form_values_are_normalized() {
        let config = Config::default();
        let mut form = to_form(&config);
        form.volume = 7.0;
        form.poll_seconds = 1;
        form.bell_file = "   ".into();
        form.token = "  abc  ".into();
        let applied = apply_form(&config, &form);
        assert_eq!(applied.sound.volume, 1.0);
        assert_eq!(applied.alert.poll_seconds, 10);
        assert_eq!(applied.sound.bell_file, None);
        assert_eq!(applied.alert.token, "abc");
    }

    #[test]
    fn region_index_maps_to_uid() {
        let config = Config::default();
        let mut form = to_form(&config);
        assert_eq!(REGIONS[form.region as usize].uid, config.alert.region_uid);
        form.region = regions::index_of(14).unwrap() as i32;
        assert_eq!(apply_form(&config, &form).alert.region_uid, 14);
    }
}
