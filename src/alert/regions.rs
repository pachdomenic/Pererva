//! Довідник областей і районів України з ідентифікаторами alerts.in.ua.

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

/// Район області. Тривогу по району вміє повертати лише alerts.in.ua.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct District {
    /// Ідентифікатор за класифікацією alerts.in.ua.
    pub uid: u32,
    /// Ідентифікатор області, до якої належить район.
    pub region_uid: u32,
    pub name: &'static str,
}

const fn district(uid: u32, region_uid: u32, name: &'static str) -> District {
    District {
        uid,
        region_uid,
        name,
    }
}

/// Райони за областями, в межах області — в алфавітному порядку.
/// Джерело: таблиця UID, на яку посилається документація devs.alerts.in.ua.
/// Для АР Крим, м. Київ і м. Севастополь районів немає.
pub const DISTRICTS: &[District] = &[
    district(38, 8, "Володимирський район"),
    district(41, 8, "Камінь-Каширський район"),
    district(40, 8, "Ковельський район"),
    district(39, 8, "Луцький район"),
    district(36, 4, "Вінницький район"),
    district(37, 4, "Гайсинський район"),
    district(35, 4, "Жмеринський район"),
    district(33, 4, "Могилів-Подільський район"),
    district(32, 4, "Тульчинський район"),
    district(34, 4, "Хмільницький район"),
    district(44, 9, "Дніпровський район"),
    district(42, 9, "Кам'янський район"),
    district(46, 9, "Криворізький район"),
    district(47, 9, "Нікопольський район"),
    district(45, 9, "Павлоградський район"),
    district(43, 9, "Самарівський район"),
    district(48, 9, "Синельниківський район"),
    district(54, 28, "Бахмутський район"),
    district(55, 28, "Волноваський район"),
    district(51, 28, "Горлівський район"),
    district(53, 28, "Донецький район"),
    district(49, 28, "Кальміуський район"),
    district(50, 28, "Краматорський район"),
    district(52, 28, "Маріупольський район"),
    district(56, 28, "Покровський район"),
    district(57, 10, "Бердичівський район"),
    district(59, 10, "Житомирський район"),
    district(60, 10, "Звягельський район"),
    district(58, 10, "Коростенський район"),
    district(61, 11, "Берегівський район"),
    district(65, 11, "Мукачівський район"),
    district(63, 11, "Рахівський район"),
    district(64, 11, "Тячівський район"),
    district(66, 11, "Ужгородський район"),
    district(62, 11, "Хустський район"),
    district(147, 12, "Бердянський район"),
    district(146, 12, "Василівський район"),
    district(149, 12, "Запорізький район"),
    district(148, 12, "Мелітопольський район"),
    district(145, 12, "Пологівський район"),
    district(73, 14, "Білоцерківський район"),
    district(78, 14, "Бориспільський район"),
    district(79, 14, "Броварський район"),
    district(75, 14, "Бучанський район"),
    district(74, 14, "Вишгородський район"),
    district(76, 14, "Обухівський район"),
    district(77, 14, "Фастівський район"),
    district(82, 15, "Голованівський район"),
    district(81, 15, "Кропивницький район"),
    district(83, 15, "Новоукраїнський район"),
    district(80, 15, "Олександрійський район"),
    district(1803, 16, "Алчевський район"),
    district(1804, 16, "Довжанський район"),
    district(1801, 16, "Луганський район"),
    district(1802, 16, "Ровеньківський район"),
    district(85, 16, "Сватівський район"),
    district(84, 16, "Сіверськодонецький район"),
    district(86, 16, "Старобільський район"),
    district(87, 16, "Щастинський район"),
    district(91, 27, "Дрогобицький район"),
    district(94, 27, "Золочівський район"),
    district(90, 27, "Львівський район"),
    district(88, 27, "Самбірський район"),
    district(89, 27, "Стрийський район"),
    district(92, 27, "Шептицький район"),
    district(93, 27, "Яворівський район"),
    district(96, 17, "Баштанський район"),
    district(95, 17, "Вознесенський район"),
    district(98, 17, "Миколаївський район"),
    district(97, 17, "Первомайський район"),
    district(100, 18, "Березівський район"),
    district(102, 18, "Білгород-Дністровський район"),
    district(105, 18, "Болградський район"),
    district(101, 18, "Ізмаїльський район"),
    district(104, 18, "Одеський район"),
    district(99, 18, "Подільський район"),
    district(103, 18, "Роздільнянський район"),
    district(107, 19, "Кременчуцький район"),
    district(106, 19, "Лубенський район"),
    district(108, 19, "Миргородський район"),
    district(109, 19, "Полтавський район"),
    district(110, 5, "Вараський район"),
    district(111, 5, "Дубенський район"),
    district(112, 5, "Рівненський район"),
    district(113, 5, "Сарненський район"),
    district(117, 20, "Конотопський район"),
    district(118, 20, "Охтирський район"),
    district(116, 20, "Роменський район"),
    district(114, 20, "Сумський район"),
    district(115, 20, "Шосткинський район"),
    district(120, 21, "Кременецький район"),
    district(119, 21, "Тернопільський район"),
    district(121, 21, "Чортківський район"),
    district(127, 22, "Берестинський район"),
    district(126, 22, "Богодухівський район"),
    district(125, 22, "Ізюмський район"),
    district(123, 22, "Куп'янський район"),
    district(128, 22, "Лозівський район"),
    district(124, 22, "Харківський район"),
    district(122, 22, "Чугуївський район"),
    district(129, 23, "Бериславський район"),
    district(133, 23, "Генічеський район"),
    district(131, 23, "Каховський район"),
    district(130, 23, "Скадовський район"),
    district(132, 23, "Херсонський район"),
    district(135, 3, "Кам'янець-Подільський район"),
    district(134, 3, "Хмельницький район"),
    district(136, 3, "Шепетівський район"),
    district(150, 24, "Звенигородський район"),
    district(153, 24, "Золотоніський район"),
    district(151, 24, "Уманський район"),
    district(152, 24, "Черкаський район"),
    district(138, 26, "Вижницький район"),
    district(139, 26, "Дністровський район"),
    district(137, 26, "Чернівецький район"),
    district(144, 25, "Корюківський район"),
    district(142, 25, "Ніжинський район"),
    district(141, 25, "Новгород-Сіверський район"),
    district(143, 25, "Прилуцький район"),
    district(140, 25, "Чернігівський район"),
    district(67, 13, "Верховинський район"),
    district(68, 13, "Івано-Франківський район"),
    district(71, 13, "Калуський район"),
    district(70, 13, "Коломийський район"),
    district(69, 13, "Косівський район"),
    district(72, 13, "Надвірнянський район"),
];

/// Райони області в алфавітному порядку.
pub fn districts_of(region_uid: u32) -> impl Iterator<Item = &'static District> {
    DISTRICTS.iter().filter(move |d| d.region_uid == region_uid)
}

pub fn district_by_uid(uid: u32) -> Option<&'static District> {
    DISTRICTS.iter().find(|d| d.uid == uid)
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
    fn district_uids_do_not_clash_with_regions() {
        let mut uids: Vec<_> = REGIONS
            .iter()
            .map(|r| r.uid)
            .chain(DISTRICTS.iter().map(|d| d.uid))
            .collect();
        uids.sort_unstable();
        uids.dedup();
        assert_eq!(uids.len(), REGIONS.len() + DISTRICTS.len());
    }

    #[test]
    fn every_district_belongs_to_a_region() {
        for d in DISTRICTS {
            assert!(by_uid(d.region_uid).is_some(), "{}", d.name);
        }
    }

    #[test]
    fn districts_of_region() {
        let kyiv_oblast: Vec<_> = districts_of(14).map(|d| d.name).collect();
        assert_eq!(kyiv_oblast.len(), 7);
        assert_eq!(kyiv_oblast[0], "Білоцерківський район");
        assert!(kyiv_oblast.contains(&"Бориспільський район"));
        assert_eq!(districts_of(31).count(), 0);
        assert_eq!(district_by_uid(78).unwrap().region_uid, 14);
        assert!(district_by_uid(14).is_none());
    }

    #[test]
    fn aliases_match() {
        assert!(by_uid(29).unwrap().matches("АР Крим"));
        assert!(by_uid(13).unwrap().matches("івано-франківська область"));
    }
}
