//! Довідник областей України з ідентифікаторами alerts.in.ua.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    /// Ідентифікатор за класифікацією alerts.in.ua.
    pub uid: u32,
    pub name: &'static str,
    /// Альтернативні назви, під якими область може з'являтися в інших джерелах.
    pub aliases: &'static [&'static str],
}

const fn region(uid: u32, name: &'static str) -> Region {
    Region {
        uid,
        name,
        aliases: &[],
    }
}

/// Області в алфавітному порядку.
pub const REGIONS: &[Region] = &[
    Region {
        uid: 29,
        name: "Автономна Республіка Крим",
        aliases: &["АР Крим", "Крим"],
    },
    region(4, "Вінницька область"),
    region(8, "Волинська область"),
    region(9, "Дніпропетровська область"),
    region(28, "Донецька область"),
    region(10, "Житомирська область"),
    region(11, "Закарпатська область"),
    region(12, "Запорізька область"),
    region(13, "Івано-Франківська область"),
    region(14, "Київська область"),
    region(15, "Кіровоградська область"),
    region(16, "Луганська область"),
    region(27, "Львівська область"),
    region(17, "Миколаївська область"),
    region(18, "Одеська область"),
    region(19, "Полтавська область"),
    region(5, "Рівненська область"),
    region(20, "Сумська область"),
    region(21, "Тернопільська область"),
    region(22, "Харківська область"),
    region(23, "Херсонська область"),
    region(3, "Хмельницька область"),
    region(24, "Черкаська область"),
    region(26, "Чернівецька область"),
    region(25, "Чернігівська область"),
    region(31, "м. Київ"),
    Region {
        uid: 30,
        name: "м. Севастополь",
        aliases: &["Севастополь"],
    },
];

pub fn by_uid(uid: u32) -> Option<&'static Region> {
    REGIONS.iter().find(|r| r.uid == uid)
}

pub fn index_of(uid: u32) -> Option<usize> {
    REGIONS.iter().position(|r| r.uid == uid)
}

/// Нормалізує назву для порівняння: нижній регістр, без «м.», «область», «обл.».
pub fn normalize(name: &str) -> String {
    let lower = name.to_lowercase().replace(['’', 'ʼ', '`'], "'");
    lower
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| {
            !w.is_empty() && !matches!(*w, "м." | "м" | "місто" | "область" | "обл." | "обл")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl Region {
    /// Чи відповідає назва з зовнішнього джерела цій області.
    pub fn matches(&self, other: &str) -> bool {
        let other = normalize(other);
        normalize(self.name) == other || self.aliases.iter().any(|a| normalize(a) == other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uids_are_unique() {
        let mut uids: Vec<_> = REGIONS.iter().map(|r| r.uid).collect();
        uids.sort_unstable();
        uids.dedup();
        assert_eq!(uids.len(), REGIONS.len());
    }

    #[test]
    fn lookup_by_uid() {
        assert_eq!(by_uid(31).unwrap().name, "м. Київ");
        assert_eq!(by_uid(14).unwrap().name, "Київська область");
        assert!(by_uid(1).is_none());
    }

    #[test]
    fn kyiv_city_and_oblast_do_not_mix() {
        let city = by_uid(31).unwrap();
        let oblast = by_uid(14).unwrap();
        assert!(city.matches("м. Київ"));
        assert!(!city.matches("Київська область"));
        assert!(oblast.matches("Київська обл."));
        assert!(!oblast.matches("м. Київ"));
    }

    #[test]
    fn aliases_match() {
        assert!(by_uid(29).unwrap().matches("АР Крим"));
        assert!(by_uid(13).unwrap().matches("івано-франківська область"));
    }
}
