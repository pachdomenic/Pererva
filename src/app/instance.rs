//! Один екземпляр програми на користувача. Вікно можна сховати в трей, і
//! тоді легко запустити програму вдруге — дзвінки й сирена звучали б двічі.
//! Тому другий запуск лише просить перший показати вікно і завершується.
//!
//! Перший екземпляр тримає блокування файла `instance.lock` і слухає
//! локальний TCP-порт, номер якого записано в `instance.port` (окремий файл:
//! у Windows заблокований файл не прочитати іншому процесу).

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const LOCK_FILE: &str = "instance.lock";
const PORT_FILE: &str = "instance.port";
const SHOW: &[u8] = b"show\n";

/// Результат спроби стати єдиним екземпляром.
pub enum Instance {
    /// Цей процес — перший; блокування діє, доки живе значення.
    Primary(Guard),
    /// Програма вже працює; всередині — чи вдалося попросити її показати вікно.
    AlreadyRunning(io::Result<()>),
}

pub struct Guard {
    _lock: File,
    listener: TcpListener,
}

/// Каталог для файлів блокування.
pub fn default_dir() -> PathBuf {
    dirs::runtime_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("shkilni-dzvinky")
}

pub fn acquire(dir: &Path) -> io::Result<Instance> {
    fs::create_dir_all(dir)?;
    let lock = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(dir.join(LOCK_FILE))?;
    let port_path = dir.join(PORT_FILE);
    match lock.try_lock() {
        Ok(()) => {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
            fs::write(&port_path, listener.local_addr()?.port().to_string())?;
            Ok(Instance::Primary(Guard {
                _lock: lock,
                listener,
            }))
        }
        Err(TryLockError::WouldBlock) => Ok(Instance::AlreadyRunning(request_show(&port_path))),
        Err(TryLockError::Error(err)) => Err(err),
    }
}

/// Надсилає першому екземпляру прохання показати вікно.
fn request_show(port_path: &Path) -> io::Result<()> {
    // Перший екземпляр міг щойно стартувати і ще не записати порт,
    // а у файлі може лишатися порт від попереднього запуску.
    let mut last_error = io::Error::new(io::ErrorKind::NotFound, "порт не записано");
    for _ in 0..20 {
        let port = fs::read_to_string(port_path)
            .ok()
            .and_then(|text| text.trim().parse::<u16>().ok());
        if let Some(port) = port {
            let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
            match TcpStream::connect_timeout(&addr, Duration::from_secs(1)) {
                Ok(mut stream) => return stream.write_all(SHOW),
                Err(err) => last_error = err,
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(last_error)
}

impl Guard {
    /// Слухає прохання інших запусків у фоновому потоці.
    pub fn listen(self, on_show: impl Fn() + Send + 'static) {
        thread::spawn(move || {
            let Guard { _lock, listener } = self;
            for stream in listener.incoming().flatten() {
                let mut buf = [0; SHOW.len()];
                let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                if (&stream).read_exact(&mut buf).is_ok() && buf == SHOW {
                    on_show();
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn second_instance_asks_first_to_show() {
        let dir = tempfile::tempdir().unwrap();
        let Instance::Primary(guard) = acquire(dir.path()).unwrap() else {
            panic!("перший запуск має стати основним");
        };
        let (tx, rx) = mpsc::channel();
        guard.listen(move || {
            let _ = tx.send(());
        });

        assert!(matches!(
            acquire(dir.path()).unwrap(),
            Instance::AlreadyRunning(Ok(()))
        ));
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }
}
