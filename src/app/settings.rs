//! Перетворення між налаштуваннями застосунку і даними форми інтерфейсу.

use crate::SettingsData;
use crate::alert::regions::{self, REGIONS};
use crate::config::{AlertProvider, Config, Language, ThemeMode, YellowMode};
use crate::i18n::pick;
use crate::schedule::{format_hhmm, parse_hhmm};
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

/// Назви областей мовою інтерфейсу.
pub fn region_names() -> Vec<slint::SharedString> {
    REGIONS.iter().map(|r| r.title().into()).collect()
}

/// Пункти списку районів: «Уся область», далі райони області.
pub fn district_names(region_uid: u32) -> Vec<slint::SharedString> {
    std::iter::once(pick("Уся область", "Entire region").to_owned())
        .chain(regions::districts_of(region_uid).map(|d| d.title()))
        .map(Into::into)
        .collect()
}

/// Номер пункту в списку районів (0 — уся область).
fn district_index(region_uid: u32, district_uid: Option<u32>) -> i32 {
    district_uid
        .and_then(|uid| regions::districts_of(region_uid).position(|d| d.uid == uid))
        .map_or(0, |i| i as i32 + 1)
}

pub fn to_form(config: &Config) -> SettingsData {
    SettingsData {
        alert_enabled: config.alert.enabled,
        provider: config.alert.provider.index() as i32,
        token: config.alert.token.as_str().into(),
        region: regions::index_of(config.alert.region_uid).unwrap_or(0) as i32,
        district: district_index(config.alert.region_uid, config.alert.district_uid),
        poll_seconds: config.alert.poll_seconds as i32,
        mute_bells: config.alert.mute_bells_during_alert,
        partial_is_alert: config.alert.partial_is_alert,
        yellow_mode: config.alert.yellow.index() as i32,
        play_all_clear: config.alert.play_all_clear,
        volume: config.sound.volume,
        bell_seconds: config.sound.bell_seconds as i32,
        siren_seconds: config.sound.siren_seconds as i32,
        bell_file: path_to_string(&config.sound.bell_file),
        siren_file: path_to_string(&config.sound.siren_file),
        all_clear_file: path_to_string(&config.sound.all_clear_file),
        yellow_siren_file: path_to_string(&config.sound.yellow_siren_file),
        yellow_all_clear_file: path_to_string(&config.sound.yellow_all_clear_file),
        memorial_enabled: config.memorial.enabled,
        memorial_time: format_hhmm(config.memorial.time).into(),
        memorial_school_days: config.memorial.school_days_only,
        memorial_anthem: config.memorial.anthem,
        silence_file: path_to_string(&config.sound.silence_file),
        anthem_file: path_to_string(&config.sound.anthem_file),
        theme: config.ui.theme.index(),
        language: config.ui.language.index(),
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
    alert.district_uid = usize::try_from(form.district)
        .ok()
        .and_then(|i| i.checked_sub(1))
        .and_then(|i| regions::districts_of(alert.region_uid).nth(i))
        .map(|d| d.uid);
    alert.poll_seconds = form.poll_seconds.max(0) as u64;
    alert.mute_bells_during_alert = form.mute_bells;
    alert.partial_is_alert = form.partial_is_alert;
    alert.yellow = YellowMode::from_index(form.yellow_mode.max(0) as usize);
    alert.play_all_clear = form.play_all_clear;

    let sound = &mut config.sound;
    sound.volume = form.volume;
    sound.bell_seconds = form.bell_seconds.max(0) as u32;
    sound.siren_seconds = form.siren_seconds.max(0) as u32;
    sound.bell_file = string_to_path(&form.bell_file);
    sound.siren_file = string_to_path(&form.siren_file);
    sound.all_clear_file = string_to_path(&form.all_clear_file);
    sound.yellow_siren_file = string_to_path(&form.yellow_siren_file);
    sound.yellow_all_clear_file = string_to_path(&form.yellow_all_clear_file);
    sound.silence_file = string_to_path(&form.silence_file);
    sound.anthem_file = string_to_path(&form.anthem_file);

    let memorial = &mut config.memorial;
    memorial.enabled = form.memorial_enabled;
    // Неправильно введений час не застосовується (поле підсвічується в інтерфейсі).
    memorial.time = parse_hhmm(&form.memorial_time).unwrap_or(memorial.time);
    memorial.school_days_only = form.memorial_school_days;
    memorial.anthem = form.memorial_anthem;

    config.ui.theme = ThemeMode::from_index(form.theme);
    config.ui.language = Language::from_index(form.language);
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
        config.alert.district_uid = Some(90);
        config.alert.yellow = YellowMode::Separate;
        config.sound.yellow_all_clear_file = Some("/tmp/yellow-clear.wav".into());
        config.sound.siren_file = Some("/tmp/siren.mp3".into());
        config.sound.anthem_file = Some("/tmp/anthem.ogg".into());
        config.memorial.time = chrono::NaiveTime::from_hms_opt(9, 5, 0).unwrap();
        config.memorial.school_days_only = true;
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

    #[test]
    fn district_index_maps_to_uid() {
        let mut config = Config::default();
        config.alert.region_uid = 14;
        let mut form = to_form(&config);
        assert_eq!(form.district, 0);

        let names = district_names(14);
        assert_eq!(names[0], "Уся область");
        form.district = names
            .iter()
            .position(|n| n == "Бориспільський район")
            .unwrap() as i32;
        let applied = apply_form(&config, &form);
        assert_eq!(applied.alert.district_uid, Some(78));
        assert_eq!(to_form(&applied).district, form.district);

        form.district = 0;
        assert_eq!(apply_form(&config, &form).alert.district_uid, None);
        form.district = 999;
        assert_eq!(apply_form(&config, &form).alert.district_uid, None);
    }

    #[test]
    fn memorial_time_is_parsed() {
        let config = Config::default();
        let mut form = to_form(&config);
        assert_eq!(form.memorial_time, "09:00");
        form.memorial_time = " 9:30 ".into();
        assert_eq!(
            apply_form(&config, &form).memorial.time,
            parse_hhmm("09:30").unwrap()
        );
        form.memorial_time = "дев'ята".into();
        assert_eq!(
            apply_form(&config, &form).memorial.time,
            config.memorial.time
        );
    }

    #[test]
    fn regions_without_districts() {
        assert_eq!(
            district_names(31),
            vec![slint::SharedString::from("Уся область")]
        );
    }
}
