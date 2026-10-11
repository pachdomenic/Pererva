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

const WHOLE_REGION: &str = "Уся область";

/// Пункти списку районів: «Уся область», далі райони області.
pub fn district_names(region_uid: u32) -> Vec<slint::SharedString> {
    std::iter::once(WHOLE_REGION)
        .chain(regions::districts_of(region_uid).map(|d| d.name))
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
    alert.district_uid = usize::try_from(form.district)
        .ok()
        .and_then(|i| i.checked_sub(1))
        .and_then(|i| regions::districts_of(alert.region_uid).nth(i))
        .map(|d| d.uid);
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
        config.alert.district_uid = Some(90);
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

    #[test]
    fn district_index_maps_to_uid() {
        let mut config = Config::default();
        config.alert.region_uid = 14;
        let mut form = to_form(&config);
        assert_eq!(form.district, 0);

        let names = district_names(14);
        assert_eq!(names[0], WHOLE_REGION);
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
    fn regions_without_districts() {
        assert_eq!(
            district_names(31),
            vec![slint::SharedString::from(WHOLE_REGION)]
        );
    }
}
