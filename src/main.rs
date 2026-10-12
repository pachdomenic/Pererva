// У релізній збірці під Windows не відкривати консольне вікно.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod alert;
mod app;
mod audio;
mod config;
mod history;
mod i18n;
mod schedule;

use app::instance::{self, Instance};
use i18n::pick;
use std::path::PathBuf;
use std::process::ExitCode;

slint::include_modules!();

const USAGE: &str = "\
Шкільні дзвінки — подача дзвінків за розкладом з урахуванням повітряної тривоги.

Використання: shkilni-dzvinky [ПАРАМЕТРИ]

Параметри:
  -c, --config <ФАЙЛ>  шлях до файлу налаштувань
  -h, --help           показати цю довідку
  -V, --version        показати версію";

const USAGE_EN: &str = "\
School Bells — rings school bells on schedule, taking air raid alerts into account.

Usage: shkilni-dzvinky [OPTIONS]

Options:
  -c, --config <FILE>  path to the settings file
  -h, --help           show this help
  -V, --version        show the version";

fn main() -> ExitCode {
    i18n::set(config::Language::System.resolve());
    let usage = pick(USAGE, USAGE_EN);
    let mut config_path = config::Config::default_path();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-c" | "--config" => match args.next() {
                Some(path) => config_path = PathBuf::from(path),
                None => {
                    eprintln!(
                        "{}\n\n{usage}",
                        tr!(
                            "Параметр {arg} потребує шляху до файлу",
                            "Option {arg} requires a file path"
                        )
                    );
                    return ExitCode::from(2);
                }
            },
            "-h" | "--help" => {
                println!("{usage}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("shkilni-dzvinky {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!(
                    "{}\n\n{usage}",
                    tr!("Невідомий параметр: {other}", "Unknown option: {other}")
                );
                return ExitCode::from(2);
            }
        }
    }

    let instance = match instance::acquire(&instance::default_dir()) {
        Ok(Instance::Primary(guard)) => Some(guard),
        Ok(Instance::AlreadyRunning(shown)) => {
            // Другий екземпляр подавав би ті самі дзвінки вдруге.
            match shown {
                Ok(()) => println!(
                    "{}",
                    pick(
                        "Програма вже запущена — показано її вікно",
                        "The app is already running — its window is shown"
                    )
                ),
                Err(err) => eprintln!(
                    "{}",
                    tr!(
                        "Програма вже запущена, але не вдалося показати її вікно: {err}",
                        "The app is already running, but its window could not be shown: {err}"
                    )
                ),
            }
            return ExitCode::SUCCESS;
        }
        Err(err) => {
            eprintln!(
                "{}",
                tr!(
                    "Не вдалося перевірити, чи програма вже запущена: {err}",
                    "Could not check whether the app is already running: {err}"
                )
            );
            None
        }
    };

    match app::controller::run(config_path, instance) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!(
                "{}",
                tr!(
                    "Не вдалося запустити інтерфейс: {err}",
                    "Could not start the interface: {err}"
                )
            );
            ExitCode::FAILURE
        }
    }
}
