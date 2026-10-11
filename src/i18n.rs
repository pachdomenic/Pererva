//! Мова інтерфейсу: українська (мова вихідних текстів) або англійська.
//!
//! Тексти розмітки Slint перекладаються через `@tr()` і файл
//! `lang/en/LC_MESSAGES/shkilni-dzvinky.po`; тексти, які складає Rust-код,
//! задаються парами «українською / англійською» через [`pick`] і [`tr!`].

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    Uk,
    En,
}

impl Lang {
    /// Мова системи: англійська для будь-якої локалі, крім української і
    /// російської; якщо локаль невідома — українська.
    pub fn from_system() -> Self {
        sys_locale::get_locale().map_or(Lang::Uk, |locale| Self::from_locale(&locale))
    }

    fn from_locale(locale: &str) -> Self {
        let code = locale.split(['-', '_', '.']).next().unwrap_or("");
        match code.to_ascii_lowercase().as_str() {
            "uk" | "ru" | "" => Lang::Uk,
            _ => Lang::En,
        }
    }

    /// Мова для `slint::select_bundled_translation`: порожній рядок — вихідні
    /// (українські) тексти розмітки.
    pub fn slint_code(self) -> &'static str {
        match self {
            Lang::Uk => "",
            Lang::En => "en",
        }
    }
}

// Мову читають і фонові потоки (тексти помилок опитування тривоги, звуку),
// тому вона глобальна. У тестах — своя для кожного потоку, щоб тести різними
// мовами не заважали один одному.
#[cfg(not(test))]
static CURRENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
thread_local! {
    static CURRENT: std::cell::Cell<Lang> = const { std::cell::Cell::new(Lang::Uk) };
}

#[cfg(not(test))]
pub fn current() -> Lang {
    if CURRENT.load(std::sync::atomic::Ordering::Relaxed) {
        Lang::En
    } else {
        Lang::Uk
    }
}

#[cfg(not(test))]
pub fn set(lang: Lang) {
    CURRENT.store(lang == Lang::En, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub fn current() -> Lang {
    CURRENT.with(|c| c.get())
}

#[cfg(test)]
pub fn set(lang: Lang) {
    CURRENT.with(|c| c.set(lang));
}

/// Вибирає варіант для поточної мови.
pub fn pick<T>(uk: T, en: T) -> T {
    match current() {
        Lang::Uk => uk,
        Lang::En => en,
    }
}

/// `format!` з окремими шаблонами для кожної мови:
/// `tr!("{n}-й урок", "Lesson {n}")`, `tr!("Помилка: {}", "Error: {}", err)`.
#[macro_export]
macro_rules! tr {
    ($uk:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        match $crate::i18n::current() {
            $crate::i18n::Lang::Uk => format!($uk $(, $arg)*),
            $crate::i18n::Lang::En => format!($en $(, $arg)*),
        }
    };
}

/// Транслітерація українського тексту латиницею за офіційною системою
/// (постанова КМУ № 55 від 27.01.2010): «Бориспільський» → «Boryspilskyi».
pub fn transliterate(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    let mut word_start = true;
    let mut prev = None;
    for c in text.chars() {
        let lower = c.to_lowercase().next().unwrap_or(c);
        let latin = match lower {
            'а' => "a",
            'б' => "b",
            'в' => "v",
            // «зг» передається як «zgh», щоб не плутати з «ж».
            'г' if prev == Some('з') => "gh",
            'г' => "h",
            'ґ' => "g",
            'д' => "d",
            'е' => "e",
            'є' if word_start => "ye",
            'є' => "ie",
            'ж' => "zh",
            'з' => "z",
            'и' => "y",
            'і' => "i",
            'ї' if word_start => "yi",
            'ї' => "i",
            'й' if word_start => "y",
            'й' => "i",
            'к' => "k",
            'л' => "l",
            'м' => "m",
            'н' => "n",
            'о' => "o",
            'п' => "p",
            'р' => "r",
            'с' => "s",
            'т' => "t",
            'у' => "u",
            'ф' => "f",
            'х' => "kh",
            'ц' => "ts",
            'ч' => "ch",
            'ш' => "sh",
            'щ' => "shch",
            'ю' if word_start => "yu",
            'ю' => "iu",
            'я' if word_start => "ya",
            'я' => "ia",
            'ь' | '\'' | '’' | 'ʼ' => "",
            _ => {
                out.push(c);
                word_start = !c.is_alphabetic();
                prev = None;
                continue;
            }
        };
        if c.is_uppercase() {
            let mut chars = latin.chars();
            if let Some(first) = chars.next() {
                out.extend(first.to_uppercase());
                out.push_str(chars.as_str());
            }
        } else {
            out.push_str(latin);
        }
        word_start = false;
        prev = Some(lower);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_locales() {
        assert_eq!(Lang::from_locale("uk-UA"), Lang::Uk);
        assert_eq!(Lang::from_locale("uk_UA.UTF-8"), Lang::Uk);
        assert_eq!(Lang::from_locale("ru-RU"), Lang::Uk);
        assert_eq!(Lang::from_locale("en-US"), Lang::En);
        assert_eq!(Lang::from_locale("de"), Lang::En);
    }

    #[test]
    fn language_is_switchable() {
        assert_eq!(pick("так", "yes"), "так");
        set(Lang::En);
        assert_eq!(pick("так", "yes"), "yes");
        let n = 3;
        assert_eq!(tr!("{n}-й урок", "Lesson {n}"), "Lesson 3");
        set(Lang::Uk);
        assert_eq!(tr!("{}-й урок", "Lesson {}", n), "3-й урок");
    }

    #[test]
    fn transliteration() {
        assert_eq!(transliterate("Бориспільський"), "Boryspilskyi");
        assert_eq!(transliterate("Новгород-Сіверський"), "Novhorod-Siverskyi");
        assert_eq!(transliterate("Івано-Франківський"), "Ivano-Frankivskyi");
        assert_eq!(transliterate("Ясинуватський"), "Yasynuvatskyi");
        assert_eq!(transliterate("Згурівка"), "Zghurivka");
        assert_eq!(transliterate("Їжакевич"), "Yizhakevych");
        assert_eq!(transliterate("Подільський"), "Podilskyi");
    }
}
