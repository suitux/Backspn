use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    English,
    Spanish,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::English, Self::Spanish];

    pub fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Spanish => "Español",
        }
    }

    fn code(self) -> u8 {
        match self {
            Self::English => 0,
            Self::Spanish => 1,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(language: Language) {
    CURRENT.store(language.code(), Ordering::Relaxed);
}

fn parse(file: &'static str) -> HashMap<&'static str, &'static str> {
    file.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
        .collect()
}

fn spanish() -> &'static HashMap<&'static str, &'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| parse(include_str!("../lang/es.tsv")))
}

pub fn translate(language: Language, text: &str) -> &str {
    match language {
        Language::English => text,
        Language::Spanish => spanish().get(text).copied().unwrap_or(text),
    }
}

pub fn tr(text: &str) -> &str {
    let language = if CURRENT.load(Ordering::Relaxed) == Language::Spanish.code() {
        Language::Spanish
    } else {
        Language::English
    };
    translate(language, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanish_translates_what_it_has_and_keeps_the_rest() {
        assert_eq!(translate(Language::Spanish, "Theme"), "Tema");
        assert_eq!(
            translate(Language::Spanish, "Kontrol S4 MK3"),
            "Kontrol S4 MK3"
        );
        assert_eq!(translate(Language::English, "Theme"), "Theme");
    }

    #[test]
    fn the_file_has_no_broken_lines() {
        let file = include_str!("../lang/es.tsv");
        let mut seen = std::collections::HashSet::new();
        for line in file
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
        {
            let (en, es) = line.split_once('\t').expect(line);
            assert!(
                !en.is_empty() && !es.is_empty() && !es.contains('\t'),
                "{line}"
            );
            assert!(seen.insert(en), "twice: {en}");
        }
    }
}
