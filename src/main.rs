// У релізній збірці під Windows не відкривати консольне вікно.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod alert;
mod app;
mod audio;
mod config;
mod schedule;

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

fn main() -> ExitCode {
    let mut config_path = config::Config::default_path();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-c" | "--config" => match args.next() {
                Some(path) => config_path = PathBuf::from(path),
                None => {
                    eprintln!("Параметр {arg} потребує шляху до файлу\n\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("shkilni-dzvinky {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("Невідомий параметр: {other}\n\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }

    match app::controller::run(config_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Не вдалося запустити інтерфейс: {err}");
            ExitCode::FAILURE
        }
    }
}
