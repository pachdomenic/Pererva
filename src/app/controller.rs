//! Контролер: подає дзвінки за розкладом, реагує на тривогу, оновлює інтерфейс
//! і обробляє дії користувача.

use super::draft::ScheduleDraft;
use super::format::{countdown, duration_words, long_date, ordinal, relative_day};
use super::instance::Guard;
use super::settings::{apply_form, district_names, region_names, to_form};
use crate::alert::monitor::{AlertMonitor, AlertUpdate};
use crate::alert::tracker::{AlertTracker, Transition};
use crate::alert::{AlertStatus, Location, ThreatLevel};
use crate::audio::{Player, Sound};
use crate::config::{AlertProvider, Config, Language, ThemeMode, YellowMode};
use crate::i18n::{self, Lang, pick};
use crate::schedule::{BellEvent, BellKind, DayStatus, format_hhmm, parse_hhmm};
use crate::{
    AlertLevel, AppState, AppTray, AppWindow, EditLesson, LessonRow, LessonState, LogEntry,
    LogKind, Theme, tr,
};
use chrono::{DateTime, Duration, Local, NaiveDateTime};
use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration as StdDuration, Instant};

/// Період оновлення інтерфейсу і перевірки дзвінків.
const TICK: StdDuration = StdDuration::from_millis(250);
/// Якщо годинник «перестрибнув» далі (сон ПК), пропущені дзвінки не подаються.
const MAX_CATCH_UP_SECONDS: i64 = 120;
const LOG_LIMIT: usize = 50;
const TOAST_DURATION: StdDuration = StdDuration::from_secs(4);

/// Повідомлення від фонових потоків.
enum Event {
    Alert(AlertUpdate),
    AudioError(String),
}

pub struct Controller {
    window: slint::Weak<AppWindow>,
    tray: slint::Weak<AppTray>,
    config: Config,
    config_path: PathBuf,
    player: Player,
    monitor: AlertMonitor,
    events: Receiver<Event>,
    tracker: AlertTracker,
    last_update: Option<AlertUpdate>,
    last_error: Option<String>,
    /// Час початку навчальної тривоги, якщо вона триває.
    drill: Option<DateTime<Local>>,
    last_tick: NaiveDateTime,
    draft: ScheduleDraft,
    log: Rc<VecModel<LogEntry>>,
    edit_lessons: Rc<VecModel<EditLesson>>,
    edit_days: Rc<VecModel<bool>>,
    today: Vec<LessonRow>,
    toast_until: Option<Instant>,
    /// Коли востаннє почалася хвилина мовчання (поки програвач не підтвердив
    /// відтворення, дзвінки все одно не подаються).
    memorial_started: Option<Instant>,
}

/// Створює вікно, запускає фонові потоки і цикл подій.
///
/// `instance` — блокування єдиного екземпляра: через нього інші запуски
/// просять показати вікно.
pub fn run(config_path: PathBuf, instance: Option<Guard>) -> Result<(), slint::PlatformError> {
    let window = AppWindow::new()?;
    // Поки налаштування не прочитано, помилки — типовою мовою.
    i18n::set(Language::default().resolve());
    // Поки значок у треї видимий, закрите вікно лише ховається, а цикл подій
    // (дзвінки, тривога, звук) працює далі. Якщо трею в системі немає,
    // значок не створюється і закриття вікна, як і раніше, завершує програму.
    let tray = AppTray::new()?;
    let (config, load_error) = match Config::load(&config_path) {
        Ok(config) => (config, None),
        Err(err) => (Config::default(), Some(format!("{err:#}"))),
    };

    let (tx, events) = mpsc::channel();
    let alert_tx = tx.clone();
    let monitor = AlertMonitor::spawn(config.alert.clone(), move |update| {
        let _ = alert_tx.send(Event::Alert(update));
    });
    let player = Player::spawn(move |message| {
        let _ = tx.send(Event::AudioError(message));
    });

    let controller = Rc::new(RefCell::new(Controller {
        window: window.as_weak(),
        tray: tray.as_weak(),
        draft: ScheduleDraft::from_schedule(&config.schedule),
        config,
        config_path,
        player,
        monitor,
        events,
        tracker: AlertTracker::default(),
        last_update: None,
        last_error: None,
        drill: None,
        last_tick: Local::now().naive_local(),
        log: Rc::new(VecModel::default()),
        edit_lessons: Rc::new(VecModel::default()),
        edit_days: Rc::new(VecModel::default()),
        today: Vec::new(),
        toast_until: None,
        memorial_started: None,
    }));
    controller.borrow_mut().init(load_error);
    bind_callbacks(&window, &controller);
    bind_tray(&tray, &window, &controller);
    if let Some(instance) = instance {
        let weak = window.as_weak();
        instance.listen(move || {
            let _ = weak.upgrade_in_event_loop(|w| show_window(&w));
        });
    }

    let timer = Timer::default();
    let weak = Rc::downgrade(&controller);
    timer.start(TimerMode::Repeated, TICK, move || {
        if let Some(controller) = weak.upgrade() {
            controller.borrow_mut().tick();
        }
    });

    window.show()?;
    slint::run_event_loop()
}

/// Показує сховане або згорнуте вікно.
fn show_window(window: &AppWindow) {
    if let Err(err) = window.show() {
        eprintln!(
            "{}",
            tr!(
                "Не вдалося показати вікно: {err}",
                "Could not show the window: {err}"
            )
        );
    }
    window.window().set_minimized(false);
}

fn bind_tray(tray: &AppTray, window: &AppWindow, controller: &Rc<RefCell<Controller>>) {
    let weak = window.as_weak();
    tray.on_show_window(move || {
        if let Some(window) = weak.upgrade() {
            show_window(&window);
        }
    });
    tray.on_stop_sound(action(controller, |c| c.player.stop()));
    tray.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
}

/// Обгортає метод контролера у колбек без аргументів.
fn action(
    controller: &Rc<RefCell<Controller>>,
    f: impl Fn(&mut Controller) + 'static,
) -> impl Fn() + 'static {
    let weak = Rc::downgrade(controller);
    move || {
        if let Some(controller) = weak.upgrade() {
            f(&mut controller.borrow_mut());
        }
    }
}

fn bind_callbacks(window: &AppWindow, controller: &Rc<RefCell<Controller>>) {
    let state = window.global::<AppState>();

    state.on_ring_bell(action(controller, Controller::ring_bell_manually));
    state.on_test_siren(action(controller, |c| c.play_manually(Sound::Siren)));
    state.on_test_all_clear(action(controller, |c| c.play_manually(Sound::AllClear)));
    state.on_stop_sound(action(controller, |c| c.player.stop()));
    state.on_refresh_alert(action(controller, |c| c.monitor.refresh()));
    state.on_toggle_drill(action(controller, Controller::toggle_drill));
    state.on_start_memorial(action(controller, |c| c.start_memorial(true)));

    state.on_add_lesson(action(controller, Controller::add_lesson));
    state.on_schedule_changed(action(controller, Controller::schedule_changed));
    state.on_save_schedule(action(controller, Controller::save_schedule));
    state.on_revert_schedule(action(controller, Controller::revert_schedule));
    state.on_settings_changed(action(controller, Controller::settings_changed));
    state.on_save_settings(action(controller, Controller::save_settings));
    state.on_dismiss_toast(action(controller, Controller::hide_toast));

    let weak = Rc::downgrade(controller);
    state.on_lesson_edited(move |index, is_end, text| {
        if let Some(c) = weak.upgrade() {
            c.borrow_mut().lesson_edited(index, is_end, &text);
        }
    });
    let weak = Rc::downgrade(controller);
    state.on_remove_lesson(move |index| {
        if let Some(c) = weak.upgrade() {
            c.borrow_mut().remove_lesson(index);
        }
    });
    let weak = Rc::downgrade(controller);
    state.on_preview_sound(move |index| {
        if let Some(c) = weak.upgrade() {
            c.borrow_mut().preview_sound(index);
        }
    });
    let weak = Rc::downgrade(controller);
    state.on_theme_changed(move |index| {
        if let Some(c) = weak.upgrade() {
            c.borrow_mut().theme_changed(index);
        }
    });
    let weak = Rc::downgrade(controller);
    state.on_language_changed(move |index| {
        if let Some(c) = weak.upgrade() {
            c.borrow_mut().language_changed(index);
        }
    });
}

/// Перемикає мову текстів Rust-коду і розмітки Slint.
fn select_language(lang: Lang) {
    i18n::set(lang);
    if let Err(err) = slint::select_bundled_translation(lang.slint_code()) {
        eprintln!("{err}");
    }
}

/// Назва тривоги. Рівні розрізняє лише alerts.in.ua (`levels`), для інших
/// джерел тривога просто «повітряна».
fn alert_name(level: ThreatLevel, levels: bool) -> &'static str {
    match (level, levels) {
        (ThreatLevel::Yellow, _) => pick("Тривога жовтого рівня", "Yellow-level alert"),
        (ThreatLevel::Red, true) => pick("Тривога червоного рівня", "Red-level alert"),
        (ThreatLevel::Red, false) => pick("Повітряна тривога", "Air raid alert"),
    }
}

/// Текст дзвінка для журналу та картки «Наступний дзвінок».
fn bell_text(event: &BellEvent) -> String {
    let n = event.lesson;
    match event.kind {
        BellKind::LessonStart => match i18n::current() {
            Lang::Uk => format!("Дзвінок на {} урок", ordinal(n)),
            Lang::En => format!("Bell for lesson {n}"),
        },
        BellKind::LessonEnd => tr!("Дзвінок з {n}-го уроку", "Bell after lesson {n}"),
        BellKind::Warning => tr!(
            "Попереджувальний дзвінок перед {n}-м уроком",
            "Warning bell before lesson {n}"
        ),
    }
}

fn progress(elapsed: Duration, total: Duration) -> f32 {
    if total <= Duration::zero() {
        return 0.0;
    }
    (elapsed.num_milliseconds() as f64 / total.num_milliseconds() as f64).clamp(0.0, 1.0) as f32
}

impl Controller {
    fn with_window(&self, f: impl FnOnce(&AppWindow)) {
        if let Some(window) = self.window.upgrade() {
            f(&window);
        }
    }

    fn init(&mut self, load_error: Option<String>) {
        let config = self.config.clone();
        let path = self.config_path.display().to_string();
        self.with_window(|w| {
            let state = w.global::<AppState>();
            state.set_config_path(path.into());
            select_language(config.ui.language.resolve());
            state.set_regions(ModelRc::new(VecModel::from(region_names())));
            state.set_districts(ModelRc::new(VecModel::from(district_names(
                config.alert.region_uid,
            ))));
            state.set_settings(to_form(&config));
            state.set_log(ModelRc::from(self.log.clone()));
            state.set_edit_days(ModelRc::from(self.edit_days.clone()));
            w.global::<Theme>().set_mode(config.ui.theme.index());
        });
        self.push_draft(true);
        self.add_log(
            LogKind::Info,
            pick("Програму запущено", "App started").into(),
        );
        if let Some(error) = load_error {
            self.add_log(
                LogKind::Error,
                tr!(
                    "Налаштування не завантажено: {error}",
                    "Settings not loaded: {error}"
                ),
            );
            self.show_toast(
                pick(
                    "Не вдалося прочитати налаштування — використано типові",
                    "Could not read settings — using defaults",
                ),
                true,
            );
        }
        self.tick();
    }

    // ---------- Цикл оновлення ----------

    fn tick(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Alert(update) => self.on_alert(update),
                Event::AudioError(message) => {
                    let text = tr!("Звук: {message}", "Sound: {message}");
                    self.add_log(LogKind::Error, text.clone());
                    self.show_toast(&text, true);
                }
            }
        }

        let now = Local::now();
        let naive = now.naive_local();
        if naive > self.last_tick {
            if (naive - self.last_tick).num_seconds() <= MAX_CATCH_UP_SECONDS {
                let config = &self.config;
                if config
                    .memorial
                    .due_between(self.last_tick, naive, &config.schedule)
                    .is_some()
                {
                    self.start_memorial(false);
                }
                for event in self.config.schedule.events_between(self.last_tick, naive) {
                    self.on_bell(&event);
                }
            }
            self.last_tick = naive;
        } else if naive < self.last_tick {
            // Годинник переведено назад — продовжуємо з нового часу.
            self.last_tick = naive;
        }

        if self
            .toast_until
            .is_some_and(|until| Instant::now() >= until)
        {
            self.hide_toast();
        }

        self.render_day(naive);
        self.render_next_bell(naive);
        self.render_alert(now);
        self.render_today(naive);
        let playing = self.player.now_playing();
        self.with_window(|w| {
            let state = w.global::<AppState>();
            state.set_sound_playing(playing.is_some());
            state.set_playing_label(playing.map(Sound::title).unwrap_or_default().into());
        });
        self.render_tray(playing.is_some());
    }

    /// Стан значка в треї: видимість, підказка і пункт «Зупинити звук».
    fn render_tray(&self, sound_playing: bool) {
        let (Some(tray), Some(window)) = (self.tray.upgrade(), self.window.upgrade()) else {
            return;
        };
        let state = window.global::<AppState>();
        let mut status = state.get_alert_title().to_string();
        if state.get_next_bell_time() != "--:--" {
            status = format!(
                "{status}\n{} {}",
                state.get_next_bell_label(),
                state.get_next_bell_in()
            );
        }
        tray.set_shown(self.config.ui.close_to_tray);
        tray.set_status(status.into());
        tray.set_sound_playing(sound_playing);
    }

    fn bells_muted(&self) -> bool {
        let real_alert = self.config.alert.enabled && self.tracker.is_active();
        self.config.alert.mute_bells_during_alert && (real_alert || self.drill.is_some())
    }

    fn on_bell(&mut self, event: &BellEvent) {
        let text = bell_text(event);
        if self.bells_muted() {
            self.add_log(
                LogKind::Info,
                tr!(
                    "{text} — пропущено через тривогу",
                    "{text} — skipped due to alert"
                ),
            );
            return;
        }
        if self.memorial_active() {
            self.add_log(
                LogKind::Info,
                tr!(
                    "{text} — пропущено через хвилину мовчання",
                    "{text} — skipped due to minute of silence"
                ),
            );
            return;
        }
        let mut sound = self.config.sound.clone();
        if event.kind == BellKind::Warning {
            sound.bell_seconds = (sound.bell_seconds / 2).max(1);
        }
        self.player.play(Sound::Bell, &sound);
        self.add_log(LogKind::Bell, text);
    }

    fn on_alert(&mut self, update: AlertUpdate) {
        if !update.enabled {
            self.tracker.reset();
            self.last_error = None;
            self.last_update = Some(update);
            return;
        }

        if update.error != self.last_error {
            match &update.error {
                Some(error) => self.add_log(
                    LogKind::Error,
                    tr!("API тривоги: {error}", "Alert API: {error}"),
                ),
                None if self.last_error.is_some() => self.add_log(
                    LogKind::Info,
                    pick(
                        "З'єднання з API тривоги відновлено",
                        "Connection to the alert API restored",
                    )
                    .into(),
                ),
                None => {}
            }
            self.last_error = update.error.clone();
        }

        let alert = update.reading.alert(&self.config.alert);
        let transition = self
            .tracker
            .update(update.reading, alert, update.checked_at);
        // Окремі звуки — лише для тривоги жовтого рівня в режимі «Окремі звуки».
        let separate = |level| {
            level == ThreatLevel::Yellow && self.config.alert.yellow == YellowMode::Separate
        };
        let region = &update.region;
        let levels = self.config.alert.provider == AlertProvider::AlertsInUa;
        match transition {
            Transition::Started(level) => {
                let siren = if separate(level) {
                    Sound::YellowSiren
                } else {
                    Sound::Siren
                };
                self.player.play(siren, &self.config.sound);
                self.add_log(
                    LogKind::Alert,
                    format!("{}: {region}", alert_name(level, levels)),
                );
            }
            Transition::Changed(ThreatLevel::Red) => {
                // Якщо тривога жовтого рівня мала власний сигнал, на червоному звучить сирена.
                if self.config.alert.yellow == YellowMode::Separate {
                    self.player.play(Sound::Siren, &self.config.sound);
                }
                self.add_log(
                    LogKind::Alert,
                    tr!(
                        "Тривогу підвищено до червоного рівня: {region}",
                        "Alert raised to red level: {region}"
                    ),
                );
            }
            Transition::Changed(ThreatLevel::Yellow) => {
                self.add_log(
                    LogKind::Info,
                    tr!(
                        "Рівень тривоги знижено до жовтого: {region}",
                        "Alert lowered to yellow level: {region}"
                    ),
                );
            }
            Transition::Ended(level) => {
                if self.config.alert.play_all_clear {
                    let all_clear = if separate(level) {
                        Sound::YellowAllClear
                    } else {
                        Sound::AllClear
                    };
                    self.player.play(all_clear, &self.config.sound);
                }
                let text = match (level, levels) {
                    (ThreatLevel::Yellow, _) => {
                        pick("Відбій тривоги жовтого рівня", "Yellow-level all clear")
                    }
                    (ThreatLevel::Red, true) => {
                        pick("Відбій тривоги червоного рівня", "Red-level all clear")
                    }
                    (ThreatLevel::Red, false) => pick("Відбій тривоги", "All clear"),
                };
                self.add_log(LogKind::AllClear, format!("{text}: {region}"));
            }
            Transition::None => {}
        }
        self.last_update = Some(update);
    }

    // ---------- Відображення ----------

    fn render_day(&self, now: NaiveDateTime) {
        let schedule = &self.config.schedule;
        let today = now.date();
        let (title, subtitle, value) = match schedule.status_at(now) {
            DayStatus::DayOff => {
                let next = schedule
                    .next_event(now)
                    .map(|e| {
                        let when = relative_day(now, e.at);
                        tr!("Найближчий урок {when}", "Next lesson {when}")
                    })
                    .unwrap_or_else(|| pick("Розклад порожній", "Schedule is empty").into());
                (pick("Вихідний", "Day off").to_owned(), next, 0.0)
            }
            DayStatus::BeforeLessons { first_start } => {
                let start = format_hhmm(first_start);
                let left = duration_words(today.and_time(first_start) - now);
                (
                    pick("До початку уроків", "Before lessons").to_owned(),
                    tr!(
                        "Перший урок о {start} · через {left}",
                        "First lesson at {start} · in {left}"
                    ),
                    0.0,
                )
            }
            DayStatus::Lesson { number, ends_at } => {
                let lesson = schedule.lessons[number - 1];
                let left = duration_words(today.and_time(ends_at) - now);
                let mut subtitle = tr!("До кінця уроку {left}", "Lesson ends in {left}");
                if let Some(next) = schedule.lessons.get(number) {
                    let pause = duration_words(next.start - ends_at);
                    subtitle += &tr!(" · далі перерва {pause}", " · then a {pause} break");
                }
                (
                    match i18n::current() {
                        Lang::Uk => format!("{} урок", ordinal(number)),
                        Lang::En => format!("Lesson {number}"),
                    },
                    subtitle,
                    progress(now - today.and_time(lesson.start), lesson.duration()),
                )
            }
            DayStatus::Break { next, next_start } => {
                let break_start = schedule.lessons[next - 2].end;
                let left = duration_words(today.and_time(next_start) - now);
                (
                    pick("Перерва", "Break").to_owned(),
                    tr!("До {next}-го уроку {left}", "Lesson {next} in {left}"),
                    progress(now - today.and_time(break_start), next_start - break_start),
                )
            }
            DayStatus::AfterLessons => (
                pick("Уроки закінчились", "Lessons are over").to_owned(),
                pick("Гарного відпочинку!", "Have a good rest!").to_owned(),
                1.0,
            ),
        };
        self.with_window(|w| {
            let state = w.global::<AppState>();
            state.set_clock(now.format("%H:%M").to_string().into());
            state.set_seconds(now.format("%S").to_string().into());
            state.set_date(long_date(today).into());
            state.set_day_title(title.into());
            state.set_day_subtitle(subtitle.into());
            state.set_day_progress(value);
            state.set_school_day(schedule.is_school_day(today));
        });
    }

    fn render_next_bell(&self, now: NaiveDateTime) {
        let schedule = &self.config.schedule;
        let next = schedule.next_event(now);
        self.with_window(|w| {
            let state = w.global::<AppState>();
            match next {
                Some(event) => {
                    let previous = schedule
                        .events_on(now.date())
                        .into_iter()
                        .rev()
                        .find(|e| e.at <= now)
                        .map(|e| e.at)
                        .unwrap_or(event.at - Duration::hours(1));
                    state.set_next_bell_time(event.at.format("%H:%M").to_string().into());
                    state.set_next_bell_label(bell_text(&event).into());
                    state.set_next_bell_countdown(countdown(event.at - now).into());
                    state.set_next_bell_in(relative_day(now, event.at).into());
                    state.set_next_bell_progress(progress(now - previous, event.at - previous));
                }
                None => {
                    state.set_next_bell_time("--:--".into());
                    state.set_next_bell_label(pick("Розклад порожній", "Schedule is empty").into());
                    state.set_next_bell_countdown("—".into());
                    state.set_next_bell_in(
                        pick(
                            "Додайте уроки на сторінці «Розклад»",
                            "Add lessons on the Schedule page",
                        )
                        .into(),
                    );
                    state.set_next_bell_progress(0.0);
                }
            }
        });
    }

    fn render_alert(&self, now: DateTime<Local>) {
        let location = Location::from_config(&self.config.alert).map(|l| l.title());
        let region = location
            .as_deref()
            .unwrap_or(pick("Область не вибрано", "No region selected"));
        let provider = self.config.alert.provider.title();
        let levels = self.config.alert.provider == AlertProvider::AlertsInUa;
        let tracker = &self.tracker;
        let disabled = || {
            (
                AlertLevel::Disabled,
                pick("Моніторинг вимкнено", "Monitoring is off"),
                pick(
                    "Увімкніть стеження за тривогою в налаштуваннях",
                    "Turn on alert monitoring in the settings",
                )
                .to_owned(),
                false,
            )
        };

        let (level, title, detail, error) = match &self.last_update {
            _ if !self.config.alert.enabled => disabled(),
            None => (
                AlertLevel::Unknown,
                pick("Отримання даних…", "Fetching data…"),
                String::new(),
                false,
            ),
            Some(update) if !update.enabled => disabled(),
            Some(update) => {
                let known = tracker.last_known();
                let yellow = known.is_yellow();
                let level = if tracker.is_active() {
                    AlertLevel::Active
                } else {
                    match known.status {
                        AlertStatus::Partial => AlertLevel::Partial,
                        AlertStatus::Clear => AlertLevel::Clear,
                        // Тривога жовтого рівня, на яку не треба реагувати, — лише попередження.
                        AlertStatus::Active if yellow => AlertLevel::Partial,
                        AlertStatus::Active => AlertLevel::Active,
                        AlertStatus::Unknown => AlertLevel::Unknown,
                    }
                };
                let title = match (known.status, known.level) {
                    (AlertStatus::Active, level) => alert_name(level, levels),
                    (AlertStatus::Partial, ThreatLevel::Yellow) => pick(
                        "Тривога жовтого рівня в частині громад",
                        "Yellow-level alert in some communities",
                    ),
                    (AlertStatus::Partial, ThreatLevel::Red) if levels => pick(
                        "Тривога червоного рівня в частині громад",
                        "Red-level alert in some communities",
                    ),
                    (AlertStatus::Partial, ThreatLevel::Red) => {
                        pick("Тривога в частині громад", "Alert in some communities")
                    }
                    (AlertStatus::Clear, _) => pick("Тривоги немає", "No alert"),
                    (AlertStatus::Unknown, _) => pick("Немає даних", "No data"),
                };
                let detail = match &update.error {
                    Some(error) => tr!("Помилка: {error}", "Error: {error}"),
                    None => {
                        let checked = update.checked_at.format("%H:%M:%S");
                        tr!(
                            "Оновлено о {checked} · {provider}",
                            "Updated at {checked} · {provider}"
                        )
                    }
                };
                (level, title, detail, update.error.is_some())
            }
        };
        let yellow = self.drill.is_none()
            && self.config.alert.enabled
            && self.last_update.as_ref().is_some_and(|u| u.enabled)
            && tracker.last_known().is_yellow();

        // Навчальна тривога має пріоритет у відображенні.
        let (level, title, region, detail, error, started) = match self.drill {
            Some(start) => (
                AlertLevel::Active,
                pick("Навчальна тривога", "Alert drill"),
                pick("Тренування евакуації", "Evacuation training"),
                pick(
                    "Завершіть її кнопкою «Завершити навчальну тривогу»",
                    "End it with the “End alert drill” button",
                )
                .to_owned(),
                false,
                Some(start),
            ),
            None => (level, title, region, detail, error, tracker.started_at()),
        };

        let since = if let Some(start) = started {
            let (lasts, from) = (duration_words(now - start), start.format("%H:%M"));
            tr!(
                "Триває {lasts} · з {from}",
                "Lasting {lasts} · since {from}"
            )
        } else if let Some(end) = tracker.ended_at() {
            let end = end.format("%H:%M");
            tr!("Відбій о {end}", "All clear at {end}")
        } else {
            String::new()
        };
        let muted = self.bells_muted();

        self.with_window(|w| {
            let state = w.global::<AppState>();
            state.set_alert_level(level);
            state.set_alert_yellow(yellow);
            state.set_alert_levels(levels);
            state.set_alert_title(title.into());
            state.set_alert_region(region.into());
            state.set_alert_detail(detail.into());
            state.set_alert_error(error);
            state.set_alert_since(since.into());
            state.set_bells_muted(muted);
            state.set_drill_active(self.drill.is_some());
        });
    }

    fn render_today(&mut self, now: NaiveDateTime) {
        let schedule = &self.config.schedule;
        let rows: Vec<LessonRow> = if schedule.is_school_day(now.date()) {
            let time = now.time();
            schedule
                .lessons
                .iter()
                .enumerate()
                .map(|(i, lesson)| LessonRow {
                    number: (i + 1) as i32,
                    start: format_hhmm(lesson.start).into(),
                    end: format_hhmm(lesson.end).into(),
                    state: if time >= lesson.end {
                        LessonState::Done
                    } else if time >= lesson.start {
                        LessonState::Current
                    } else {
                        LessonState::Upcoming
                    },
                })
                .collect()
        } else {
            Vec::new()
        };
        if rows != self.today {
            self.today = rows.clone();
            self.with_window(|w| {
                w.global::<AppState>()
                    .set_today(ModelRc::new(VecModel::from(rows)));
            });
        }
    }

    fn add_log(&mut self, kind: LogKind, text: String) {
        let entry = LogEntry {
            time: Local::now().format("%H:%M:%S").to_string().into(),
            text: text.into(),
            kind,
        };
        self.log.insert(0, entry);
        while self.log.row_count() > LOG_LIMIT {
            self.log.remove(self.log.row_count() - 1);
        }
    }

    fn show_toast(&mut self, text: &str, error: bool) {
        self.toast_until = Some(Instant::now() + TOAST_DURATION);
        self.with_window(|w| {
            let state = w.global::<AppState>();
            state.set_toast(text.into());
            state.set_toast_error(error);
            state.set_toast_shown(true);
        });
    }

    fn hide_toast(&mut self) {
        self.toast_until = None;
        self.with_window(|w| w.global::<AppState>().set_toast_shown(false));
    }

    // ---------- Ручне керування звуком ----------

    fn ring_bell_manually(&mut self) {
        self.player.play(Sound::Bell, &self.config.sound);
        self.add_log(
            LogKind::Bell,
            pick("Дзвінок подано вручну", "Bell rung manually").into(),
        );
    }

    fn play_manually(&mut self, sound: Sound) {
        self.player.play(sound, &self.config.sound);
        self.add_log(
            LogKind::Info,
            tr!(
                "Перевірка звуку: {}",
                "Sound test: {}",
                sound.title().to_lowercase()
            ),
        );
    }

    fn memorial_active(&self) -> bool {
        self.player.memorial_playing()
            || self
                .memorial_started
                .is_some_and(|start| start.elapsed() < StdDuration::from_secs(1))
    }

    /// Хвилина мовчання, а після неї — гімн (якщо увімкнено і вибрано файл).
    /// За розкладом не подається під час тривоги, якщо тоді вимкнено дзвінки.
    fn start_memorial(&mut self, manual: bool) {
        if !manual && self.bells_muted() {
            self.add_log(
                LogKind::Info,
                pick(
                    "Хвилину мовчання пропущено через тривогу",
                    "Minute of silence skipped due to alert",
                )
                .into(),
            );
            return;
        }
        let mut sounds = vec![Sound::Silence];
        let anthem_missing = self.config.memorial.anthem && self.config.sound.anthem_file.is_none();
        if self.config.memorial.anthem && !anthem_missing {
            sounds.push(Sound::Anthem);
        }
        let text = match (manual, sounds.len()) {
            (false, 1) => pick("Хвилина мовчання", "Minute of silence"),
            (false, _) => pick(
                "Хвилина мовчання і гімн України",
                "Minute of silence and national anthem",
            ),
            (true, 1) => pick(
                "Хвилину мовчання розпочато вручну",
                "Minute of silence started manually",
            ),
            (true, _) => pick(
                "Хвилину мовчання і гімн розпочато вручну",
                "Minute of silence and anthem started manually",
            ),
        };
        self.player.play_sequence(sounds, &self.config.sound);
        self.memorial_started = Some(Instant::now());
        self.add_log(LogKind::Memorial, text.into());
        if anthem_missing {
            self.add_log(
                LogKind::Info,
                pick(
                    "Гімн не подано: у налаштуваннях не вибрано файл гімну",
                    "Anthem not played: no anthem file selected in the settings",
                )
                .into(),
            );
        }
    }

    /// Починає або завершує навчальну тривогу (тренування евакуації).
    fn toggle_drill(&mut self) {
        match self.drill.take() {
            None => {
                self.drill = Some(Local::now());
                self.player.play(Sound::Siren, &self.config.sound);
                self.add_log(
                    LogKind::Alert,
                    pick("Навчальну тривогу розпочато", "Alert drill started").into(),
                );
            }
            Some(_) => {
                if self.config.alert.play_all_clear {
                    self.player.play(Sound::AllClear, &self.config.sound);
                } else {
                    self.player.stop();
                }
                self.add_log(
                    LogKind::AllClear,
                    pick("Навчальну тривогу завершено", "Alert drill ended").into(),
                );
            }
        }
    }

    fn preview_sound(&mut self, index: i32) {
        let sound = match index {
            0 => Sound::Bell,
            1 => Sound::Siren,
            3 => Sound::Silence,
            4 => Sound::Anthem,
            5 => Sound::YellowSiren,
            6 => Sound::YellowAllClear,
            _ => Sound::AllClear,
        };
        // Прослуховування з налаштувань форми, навіть не збережених.
        let form = self
            .window
            .upgrade()
            .map(|w| w.global::<AppState>().get_settings());
        let config = form.map_or_else(|| self.config.clone(), |f| apply_form(&self.config, &f));
        self.player.play(sound, &config.sound);
    }

    // ---------- Розклад ----------

    /// Передає чернетку розкладу в інтерфейс. `rebuild` — перебудувати список
    /// повністю (після додавання/видалення), інакше лише оновити підказки.
    fn push_draft(&mut self, rebuild: bool) {
        let (checks, result) = self.draft.check();
        if rebuild || self.edit_lessons.row_count() != checks.len() {
            let rows: Vec<EditLesson> = self
                .draft
                .rows
                .iter()
                .zip(&checks)
                .map(|(row, check)| EditLesson {
                    start: row.start.as_str().into(),
                    end: row.end.as_str().into(),
                    duration: check.note.as_str().into(),
                    start_error: check.start_error,
                    end_error: check.end_error,
                })
                .collect();
            // Нова модель, а не set_vec: поля, які користувач редагував, мають
            // розірвану прив'язку до старих даних, тож рядки треба створити заново.
            self.edit_lessons = Rc::new(VecModel::from(rows));
            self.edit_days.set_vec(self.draft.days.to_vec());
            let lessons = ModelRc::from(self.edit_lessons.clone());
            let warning = self.draft.warning_minutes as i32;
            self.with_window(|w| {
                let state = w.global::<AppState>();
                state.set_edit_lessons(lessons);
                state.set_edit_warning(warning);
            });
        } else {
            for (i, (row, check)) in self.draft.rows.iter().zip(&checks).enumerate() {
                let updated = EditLesson {
                    start: row.start.as_str().into(),
                    end: row.end.as_str().into(),
                    duration: check.note.as_str().into(),
                    start_error: check.start_error,
                    end_error: check.end_error,
                };
                if self.edit_lessons.row_data(i).as_ref() != Some(&updated) {
                    self.edit_lessons.set_row_data(i, updated);
                }
            }
        }
        let error = result.err().unwrap_or_default();
        let dirty = self.draft != ScheduleDraft::from_schedule(&self.config.schedule);
        self.with_window(|w| {
            let state = w.global::<AppState>();
            state.set_schedule_error(error.into());
            state.set_schedule_dirty(dirty);
        });
    }

    /// Зчитує з інтерфейсу дні та попереджувальний дзвінок.
    fn pull_draft_options(&mut self) {
        for (i, day) in self.draft.days.iter_mut().enumerate() {
            *day = self.edit_days.row_data(i).unwrap_or(false);
        }
        if let Some(w) = self.window.upgrade() {
            self.draft.warning_minutes = w.global::<AppState>().get_edit_warning().max(0) as u32;
        }
    }

    fn lesson_edited(&mut self, index: i32, is_end: bool, text: &str) {
        if let Some(row) = usize::try_from(index)
            .ok()
            .and_then(|i| self.draft.rows.get_mut(i))
        {
            if is_end {
                row.end = text.to_owned();
            } else {
                row.start = text.to_owned();
            }
            self.push_draft(false);
        }
    }

    fn add_lesson(&mut self) {
        self.pull_draft_options();
        self.draft.add_lesson();
        self.push_draft(true);
    }

    fn remove_lesson(&mut self, index: i32) {
        if let Ok(index) = usize::try_from(index) {
            self.pull_draft_options();
            self.draft.remove_lesson(index);
            self.push_draft(true);
        }
    }

    fn schedule_changed(&mut self) {
        self.pull_draft_options();
        self.push_draft(false);
    }

    fn save_schedule(&mut self) {
        self.pull_draft_options();
        match self.draft.check().1 {
            Ok(schedule) => {
                let mut config = self.config.clone();
                config.schedule = schedule;
                if self.persist(config) {
                    self.draft = ScheduleDraft::from_schedule(&self.config.schedule);
                    self.push_draft(true);
                    self.add_log(
                        LogKind::Info,
                        pick("Розклад змінено", "Schedule changed").into(),
                    );
                    self.show_toast(pick("Розклад збережено", "Schedule saved"), false);
                }
            }
            Err(error) => self.show_toast(&error, true),
        }
    }

    fn revert_schedule(&mut self) {
        self.draft = ScheduleDraft::from_schedule(&self.config.schedule);
        self.push_draft(true);
    }

    // ---------- Налаштування ----------

    /// Зберігає налаштування у файл; у разі помилки показує її і повертає `false`.
    fn persist(&mut self, config: Config) -> bool {
        match config.save(&self.config_path) {
            Ok(()) => {
                self.config = config;
                true
            }
            Err(err) => {
                let message = tr!(
                    "Не вдалося зберегти налаштування: {err:#}",
                    "Could not save settings: {err:#}"
                );
                self.add_log(LogKind::Error, message.clone());
                self.show_toast(&message, true);
                false
            }
        }
    }

    fn form_config(&self) -> Option<Config> {
        let window = self.window.upgrade()?;
        Some(apply_form(
            &self.config,
            &window.global::<AppState>().get_settings(),
        ))
    }

    /// Чи коректно введено час хвилини мовчання у формі.
    fn memorial_time_valid(&self) -> bool {
        self.window.upgrade().is_none_or(|w| {
            parse_hhmm(&w.global::<AppState>().get_settings().memorial_time).is_some()
        })
    }

    fn settings_changed(&mut self) {
        let config = self.form_config();
        let time_valid = self.memorial_time_valid();
        // Неправильний час теж вважається зміною, щоб було видно панель збереження.
        let dirty = !time_valid || config.as_ref().is_some_and(|c| *c != self.config);
        let region_uid = config.map_or(self.config.alert.region_uid, |c| c.alert.region_uid);
        self.with_window(|w| {
            let state = w.global::<AppState>();
            state.set_settings_dirty(dirty);
            state.set_memorial_time_valid(time_valid);
            // Список районів залежить від вибраної у формі області.
            state.set_districts(ModelRc::new(VecModel::from(district_names(region_uid))));
        });
    }

    fn save_settings(&mut self) {
        if !self.memorial_time_valid() {
            self.show_toast(
                pick(
                    "Час хвилини мовчання — у форматі ГГ:ХХ",
                    "Minute of silence time must be HH:MM",
                ),
                true,
            );
            return;
        }
        let Some(config) = self.form_config() else {
            return;
        };
        let old_alert = self.config.alert.clone();
        if !self.persist(config) {
            return;
        }
        let alert = self.config.alert.clone();
        if alert != old_alert {
            // Стан тривоги стосується конкретного місця: при його зміні
            // (область, район або джерело без підтримки районів) починаємо заново.
            if Location::from_config(&alert) != Location::from_config(&old_alert)
                || alert.enabled != old_alert.enabled
            {
                self.tracker.reset();
                self.last_update = None;
                self.last_error = None;
            }
            self.monitor.configure(alert);
        }
        let form = to_form(&self.config);
        self.with_window(|w| {
            let state = w.global::<AppState>();
            state.set_settings(form);
            state.set_settings_dirty(false);
            state.set_memorial_time_valid(true);
        });
        self.add_log(
            LogKind::Info,
            pick("Налаштування змінено", "Settings changed").into(),
        );
        self.show_toast(pick("Налаштування збережено", "Settings saved"), false);
    }

    fn theme_changed(&mut self, index: i32) {
        let theme = ThemeMode::from_index(index);
        self.with_window(|w| w.global::<Theme>().set_mode(theme.index()));
        // Тема застосовується і зберігається одразу, без кнопки «Зберегти».
        let mut config = self.config.clone();
        config.ui.theme = theme;
        self.persist(config);
        self.settings_changed();
    }

    fn language_changed(&mut self, index: i32) {
        let language = Language::from_index(index);
        select_language(language.resolve());
        // Мова, як і тема, застосовується і зберігається одразу.
        let mut config = self.config.clone();
        config.ui.language = language;
        self.persist(config);
        // Тексти, які складає Rust-код, — заново новою мовою (журнал лишається як є).
        self.with_window(|w| {
            w.global::<AppState>()
                .set_regions(ModelRc::new(VecModel::from(region_names())));
        });
        self.push_draft(false);
        self.settings_changed();
        self.tick();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 12)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    #[test]
    fn bell_texts() {
        let event = |kind| BellEvent {
            at: at(9, 0),
            kind,
            lesson: 3,
        };
        assert_eq!(
            bell_text(&event(BellKind::LessonStart)),
            "Дзвінок на 3-й урок"
        );
        assert_eq!(
            bell_text(&event(BellKind::LessonEnd)),
            "Дзвінок з 3-го уроку"
        );
        assert_eq!(
            bell_text(&event(BellKind::Warning)),
            "Попереджувальний дзвінок перед 3-м уроком"
        );
    }

    #[test]
    fn bell_texts_in_english() {
        i18n::set(Lang::En);
        let event = BellEvent {
            at: at(9, 0),
            kind: BellKind::LessonStart,
            lesson: 3,
        };
        assert_eq!(bell_text(&event), "Bell for lesson 3");
        i18n::set(Lang::Uk);
    }

    #[test]
    fn progress_is_clamped() {
        assert_eq!(
            progress(Duration::minutes(15), Duration::minutes(45)),
            1.0 / 3.0
        );
        assert_eq!(progress(Duration::minutes(-5), Duration::minutes(45)), 0.0);
        assert_eq!(progress(Duration::minutes(50), Duration::minutes(45)), 1.0);
        assert_eq!(progress(Duration::minutes(5), Duration::zero()), 0.0);
    }
}
