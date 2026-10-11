#![allow(dead_code)]

mod alert;
mod audio;
mod config;
mod schedule;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = AppWindow::new()?;
    window.run()
}
