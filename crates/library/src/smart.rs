use std::collections::HashSet;
use std::fmt;

use backspn_formats::nml::MusicalKey;

use crate::TrackRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Attribute {
    All,
    Artist,
    Bitrate,
    Bpm,
    CatalogNo,
    Color,
    Comment,
    Comment2,
    FilePath,
    FileName,
    Genre,
    Lyrics,
    ITunesUuid,
    Label,
    Mix,
    Producer,
    Release,
    Remixer,
    Title,
    ContentType,
    Key,
    KeyText,
    Id,
    PlayCount,
    ImportDate,
    ReleaseDate,
    LastPlayedDate,
    Rating,
    Time,
    Played,
    Analyzed,
    AnalysisLocked,
    FileState,
    MediaSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Tempo,
    Number,
    Rating,
    Key,
    Date,
    Duration,
    Bool,
    Choice,
}

impl Attribute {
    pub const ALL: [Self; 34] = [
        Self::All,
        Self::Artist,
        Self::Bitrate,
        Self::Bpm,
        Self::CatalogNo,
        Self::Color,
        Self::Comment,
        Self::Comment2,
        Self::FilePath,
        Self::FileName,
        Self::Genre,
        Self::Lyrics,
        Self::ITunesUuid,
        Self::Label,
        Self::Mix,
        Self::Producer,
        Self::Release,
        Self::Remixer,
        Self::Title,
        Self::ContentType,
        Self::Key,
        Self::KeyText,
        Self::Id,
        Self::PlayCount,
        Self::ImportDate,
        Self::ReleaseDate,
        Self::LastPlayedDate,
        Self::Rating,
        Self::Time,
        Self::Played,
        Self::Analyzed,
        Self::AnalysisLocked,
        Self::FileState,
        Self::MediaSource,
    ];

    pub fn token(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Artist => "ARTIST",
            Self::Bitrate => "BITRATE",
            Self::Bpm => "BPM",
            Self::CatalogNo => "CATALOG_NO",
            Self::Color => "COLOR",
            Self::Comment => "COMMENT",
            Self::Comment2 => "COMMENT2",
            Self::FilePath => "FILEPATH",
            Self::FileName => "FILENAME",
            Self::Genre => "GENRE",
            Self::Lyrics => "LYRICS",
            Self::ITunesUuid => "ITUNES_UUID",
            Self::Label => "LABEL",
            Self::Mix => "MIX",
            Self::Producer => "PRODUCER",
            Self::Release => "RELEASE",
            Self::Remixer => "REMIXER",
            Self::Title => "TITLE",
            Self::ContentType => "CONTENTTYPE",
            Self::Key => "KEY",
            Self::KeyText => "KEYTEXT",
            Self::Id => "ID",
            Self::PlayCount => "PLAYCOUNT",
            Self::ImportDate => "IMPORTDATE",
            Self::ReleaseDate => "RELEASEDATE",
            Self::LastPlayedDate => "LASTPLAYEDDATE",
            Self::Rating => "RATING",
            Self::Time => "TIME",
            Self::Played => "PLAYED",
            Self::Analyzed => "ANALYZED",
            Self::AnalysisLocked => "ANALYSISLOCKED",
            Self::FileState => "FILESTATE",
            Self::MediaSource => "MEDIASOURCE",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Artist => "Artist",
            Self::Bitrate => "Bitrate",
            Self::Bpm => "BPM",
            Self::CatalogNo => "Catalog No.",
            Self::Color => "Color",
            Self::Comment => "Comment",
            Self::Comment2 => "Comment2",
            Self::FilePath => "File Path",
            Self::FileName => "File Name",
            Self::Genre => "Genre",
            Self::Lyrics => "Lyrics",
            Self::ITunesUuid => "ITunes UUID",
            Self::Label => "Label",
            Self::Mix => "Mix",
            Self::Producer => "Producer",
            Self::Release => "Release",
            Self::Remixer => "Remixer",
            Self::Title => "Title",
            Self::ContentType => "Content Type",
            Self::Key => "Key",
            Self::KeyText => "Key Text",
            Self::Id => "ID",
            Self::PlayCount => "Play Count",
            Self::ImportDate => "Import Date",
            Self::ReleaseDate => "Release Date",
            Self::LastPlayedDate => "Last Played Date",
            Self::Rating => "Rating",
            Self::Time => "Time",
            Self::Played => "Played this Session",
            Self::Analyzed => "Analyzed",
            Self::AnalysisLocked => "Analysis Lock",
            Self::FileState => "File Availability",
            Self::MediaSource => "Media Source",
        }
    }

    pub fn kind(self) -> Kind {
        match self {
            Self::Bpm => Kind::Tempo,
            Self::Bitrate | Self::PlayCount => Kind::Number,
            Self::Rating => Kind::Rating,
            Self::Key => Kind::Key,
            Self::ImportDate | Self::ReleaseDate | Self::LastPlayedDate => Kind::Date,
            Self::Time => Kind::Duration,
            Self::Played | Self::Analyzed | Self::AnalysisLocked => Kind::Bool,
            Self::Color | Self::ContentType | Self::FileState | Self::MediaSource => Kind::Choice,
            _ => Kind::Text,
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        let token = token.to_ascii_uppercase();
        let alias = match token.as_str() {
            "ALBUM" => Some(Self::Release),
            "RANKING" => Some(Self::Rating),
            "PLAYTIME" | "LENGTH" => Some(Self::Time),
            "KEY_TEXT" => Some(Self::KeyText),
            "FILE" => Some(Self::FileName),
            _ => None,
        };
        alias.or_else(|| Self::ALL.into_iter().find(|a| a.token() == token))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Contains,
    NotContains,
    Similar,
    StartsWith,
    EndsWith,
}

impl Op {
    pub fn token(self) -> &'static str {
        match self {
            Self::Eq => "==",
            Self::Ne => "<>",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Contains => "%",
            Self::NotContains => "!%",
            Self::Similar => "~",
            Self::StartsWith => "STARTS_WITH",
            Self::EndsWith => "ENDS_WITH",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    pub op: Op,
    pub label: &'static str,
    pub relative: bool,
}

const fn choice(op: Op, label: &'static str) -> Choice {
    Choice {
        op,
        label,
        relative: false,
    }
}

pub fn choices(kind: Kind) -> &'static [Choice] {
    const TEXT: [Choice; 6] = [
        choice(Op::Contains, "contains"),
        choice(Op::NotContains, "does not contain"),
        choice(Op::Eq, "is equal to"),
        choice(Op::Ne, "is not equal to"),
        choice(Op::StartsWith, "starts with"),
        choice(Op::EndsWith, "ends with"),
    ];
    const NUMBER: [Choice; 6] = [
        choice(Op::Eq, "is equal to"),
        choice(Op::Ne, "is not equal to"),
        choice(Op::Lt, "is less than"),
        choice(Op::Gt, "is greater than"),
        choice(Op::Le, "is less than or equal to"),
        choice(Op::Ge, "is greater than or equal to"),
    ];
    const TEMPO: [Choice; 7] = [
        choice(Op::Similar, "is around"),
        choice(Op::Eq, "is equal to"),
        choice(Op::Ne, "is not equal to"),
        choice(Op::Lt, "is less than"),
        choice(Op::Gt, "is greater than"),
        choice(Op::Le, "is less than or equal to"),
        choice(Op::Ge, "is greater than or equal to"),
    ];
    const KEY: [Choice; 3] = [
        choice(Op::Eq, "is equal to"),
        choice(Op::Ne, "is not equal to"),
        choice(Op::Similar, "is matching"),
    ];
    const DATE: [Choice; 6] = [
        Choice {
            op: Op::Ge,
            label: "is in the last",
            relative: true,
        },
        Choice {
            op: Op::Lt,
            label: "is not in the last",
            relative: true,
        },
        choice(Op::Eq, "is equal to"),
        choice(Op::Ne, "is not equal to"),
        choice(Op::Lt, "is before"),
        choice(Op::Gt, "is after"),
    ];
    const BOOL: [Choice; 1] = [choice(Op::Eq, "is")];
    const CHOICE: [Choice; 2] = [choice(Op::Eq, "is"), choice(Op::Ne, "is not")];
    match kind {
        Kind::Text => &TEXT,
        Kind::Number | Kind::Rating | Kind::Duration => &NUMBER,
        Kind::Tempo => &TEMPO,
        Kind::Key => &KEY,
        Kind::Date => &DATE,
        Kind::Bool => &BOOL,
        Kind::Choice => &CHOICE,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Term {
    pub attribute: Attribute,
    pub op: Op,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    any_of: Vec<Vec<Term>>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid smartlist query at {position}: {message}")]
pub struct ParseError {
    pub position: usize,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    pub played: Option<&'a HashSet<i64>>,
    pub today: i64,
}

impl Default for Context<'_> {
    fn default() -> Self {
        let now = crate::clock::LocalTime::now();
        Self {
            played: None,
            today: days_from_civil(i64::from(now.year), now.month, now.day),
        }
    }
}

impl Query {
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut p = Parser { s: text, pos: 0 };
        let mut any_of = vec![vec![p.term()?]];
        loop {
            p.skip_ws();
            match p.peek() {
                None => break,
                Some('&') => {
                    p.pos += 1;
                    any_of.last_mut().expect("never empty").push(p.term()?);
                }
                Some('|') => {
                    p.pos += 1;
                    any_of.push(vec![p.term()?]);
                }
                Some(c) => return Err(p.error(format!("expected & or |, found {c:?}"))),
            }
        }
        Ok(Self { any_of })
    }

    pub fn parse_search(text: &str) -> Result<Option<Self>, ParseError> {
        let mut p = Parser { s: text, pos: 0 };
        let mut any_of: Vec<Vec<Term>> = vec![Vec::new()];
        loop {
            p.skip_ws();
            match p.peek() {
                None => break,
                Some('&') => p.pos += 1,
                Some('|') => {
                    p.pos += 1;
                    any_of.push(Vec::new());
                }
                Some('$') => any_of.last_mut().expect("never empty").push(p.term()?),
                Some(_) => {
                    let value = p.value()?;
                    any_of.last_mut().expect("never empty").push(Term {
                        attribute: Attribute::All,
                        op: Op::Contains,
                        value,
                    });
                }
            }
        }
        any_of.retain(|g| !g.is_empty());
        Ok((!any_of.is_empty()).then_some(Self { any_of }))
    }

    pub fn from_terms(terms: Vec<Term>, all: bool) -> Self {
        let any_of = if all {
            vec![terms]
        } else {
            terms.into_iter().map(|t| vec![t]).collect()
        };
        Self { any_of }
    }

    pub fn as_terms(&self) -> Option<(Vec<Term>, bool)> {
        if self.any_of.len() == 1 {
            return Some((self.any_of[0].clone(), true));
        }
        self.any_of
            .iter()
            .all(|g| g.len() == 1)
            .then(|| (self.any_of.iter().map(|g| g[0].clone()).collect(), false))
    }

    pub fn matches(&self, track: &TrackRow) -> bool {
        self.matches_in(track, &Context::default())
    }

    pub fn matches_in(&self, track: &TrackRow, cx: &Context) -> bool {
        self.any_of
            .iter()
            .any(|all| all.iter().all(|t| t.matches(track, cx)))
    }
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, group) in self.any_of.iter().enumerate() {
            if i > 0 {
                f.write_str(" | ")?;
            }
            for (j, t) in group.iter().enumerate() {
                if j > 0 {
                    f.write_str(" & ")?;
                }
                write!(f, "${} {} ", t.attribute.token(), t.op.token())?;
                match t.attribute.kind() {
                    Kind::Text | Kind::Choice => write!(f, "\"{}\"", t.value)?,
                    _ => f.write_str(&t.value)?,
                }
            }
        }
        Ok(())
    }
}

fn all_text(track: &TrackRow) -> [&str; 11] {
    [
        &track.title,
        &track.artist,
        &track.album,
        &track.genre,
        &track.label,
        &track.comment,
        &track.comment2,
        &track.remixer,
        &track.producer,
        &track.mix,
        &track.catalog_no,
    ]
}

fn file_name(path: &str) -> &str {
    let path = path.split('#').next().unwrap_or(path);
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

impl Term {
    fn matches(&self, track: &TrackRow, cx: &Context) -> bool {
        use Attribute as A;
        match self.attribute.kind() {
            Kind::Text => {
                let text = match self.attribute {
                    A::All => return all_text(track).iter().any(|t| self.text_matches(t)),
                    A::Artist => &track.artist,
                    A::CatalogNo => &track.catalog_no,
                    A::Comment => &track.comment,
                    A::Comment2 => &track.comment2,
                    A::FilePath => track.path.split('#').next().unwrap_or(&track.path),
                    A::FileName => file_name(&track.path),
                    A::Genre => &track.genre,
                    A::Label => &track.label,
                    A::Mix => &track.mix,
                    A::Producer => &track.producer,
                    A::Release => &track.album,
                    A::Remixer => &track.remixer,
                    A::Title => &track.title,
                    A::KeyText => &track.key_text,
                    _ => "",
                };
                self.text_matches(text)
            }
            Kind::Tempo => self.number_matches(track.bpm.unwrap_or(0.0), 2.5),
            Kind::Number => self.number_matches(
                match self.attribute {
                    A::Bitrate => f64::from(track.bitrate.unwrap_or(0) / 1000),
                    _ => f64::from(track.playcount),
                },
                0.0,
            ),
            Kind::Rating => self.number_matches(f64::from(track.rating / 51), 0.0),
            Kind::Duration => match parse_duration(&self.value) {
                Some(v) => compare(track.duration_s.unwrap_or(0.0).round(), self.op, v, 0.0),
                None => false,
            },
            Kind::Key => self.key_matches(track.musical_key),
            Kind::Date => self.date_matches(
                match self.attribute {
                    A::ImportDate => &track.import_date,
                    A::ReleaseDate => &track.release_date,
                    _ => &track.last_played,
                },
                cx,
            ),
            Kind::Bool => {
                let value = match self.attribute {
                    A::Played => cx.played.is_some_and(|p| p.contains(&track.id)),
                    A::Analyzed => track.analyzed,
                    _ => track.locked,
                };
                let wanted = !self.value.trim().eq_ignore_ascii_case("FALSE");
                (value == wanted) == (self.op != Op::Ne)
            }
            Kind::Choice => match self.attribute {
                A::Color => {
                    let wanted = color_value(&self.value);
                    let same = wanted == Some(track.color);
                    if self.op == Op::Ne { !same } else { same }
                }
                _ => self.op == Op::Ne,
            },
        }
    }

    fn number_matches(&self, lhs: f64, around: f64) -> bool {
        match self.value.trim().parse::<f64>() {
            Ok(rhs) => compare(lhs, self.op, rhs, around),
            Err(_) => self.text_matches(&lhs.to_string()),
        }
    }

    fn key_matches(&self, key: Option<u8>) -> bool {
        let Some(wanted) = MusicalKey::parse(self.value.trim())
            .map(MusicalKey::value)
            .or_else(|| self.value.trim().parse().ok().filter(|v| *v < 24))
        else {
            return false;
        };
        let Some(key) = key else {
            return self.op == Op::Ne;
        };
        match self.op {
            Op::Eq => key == wanted,
            Op::Ne => key != wanted,
            Op::Similar => match (MusicalKey::from_value(key), MusicalKey::from_value(wanted)) {
                (Some(a), Some(b)) => a.matches(b),
                _ => false,
            },
            _ => false,
        }
    }

    fn date_matches(&self, stored: &str, cx: &Context) -> bool {
        let (Some(lhs), Some(rhs)) = (
            parse_stored_date(stored),
            parse_date_value(&self.value, cx.today),
        ) else {
            return false;
        };
        compare(lhs as f64, self.op, rhs as f64, 0.0)
    }

    fn text_matches(&self, text: &str) -> bool {
        let a = text.to_lowercase();
        let b = self.value.to_lowercase();
        match self.op {
            Op::Contains => a.contains(&b),
            Op::NotContains => !a.contains(&b),
            Op::StartsWith => a.starts_with(&b),
            Op::EndsWith => a.ends_with(&b),
            Op::Eq | Op::Similar => a == b,
            Op::Ne => a != b,
            Op::Lt => a < b,
            Op::Le => a <= b,
            Op::Gt => a > b,
            Op::Ge => a >= b,
        }
    }
}

pub const COLOR_NAMES: [&str; 8] = [
    "None", "Red", "Orange", "Yellow", "Green", "Blue", "Violet", "Magenta",
];

fn color_value(text: &str) -> Option<u8> {
    let text = text.trim();
    COLOR_NAMES
        .iter()
        .position(|n| n.eq_ignore_ascii_case(text))
        .map(|i| i as u8)
        .or_else(|| text.parse().ok().filter(|v| *v < 8))
}

fn compare(lhs: f64, op: Op, rhs: f64, around: f64) -> bool {
    match op {
        Op::Eq => (lhs - rhs).abs() < 1e-6,
        Op::Ne => (lhs - rhs).abs() >= 1e-6,
        Op::Lt => lhs < rhs,
        Op::Le => lhs <= rhs,
        Op::Gt => lhs > rhs,
        Op::Ge => lhs >= rhs,
        Op::Similar => lhs >= rhs - around && lhs <= rhs + around,
        Op::Contains | Op::NotContains | Op::StartsWith | Op::EndsWith => false,
    }
}

pub fn parse_duration(text: &str) -> Option<f64> {
    let parts: Vec<u32> = text
        .trim()
        .split(':')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let (h, m, s) = match parts.as_slice() {
        [m, s] => (0, *m, *s),
        [h, m, s] => (*h, *m, *s),
        _ => return None,
    };
    (s < 60).then(|| f64::from(h * 3600 + m * 60 + s))
}

fn parse_stored_date(text: &str) -> Option<i64> {
    let mut parts = text.trim().split('/').map(str::parse::<u32>);
    let y = parts.next()?.ok()?;
    let m = parts.next().and_then(Result::ok).unwrap_or(1);
    let d = parts.next().and_then(Result::ok).unwrap_or(1);
    Some(days_from_civil(i64::from(y), m, d))
}

pub fn parse_date_value(text: &str, today: i64) -> Option<i64> {
    let text = text.trim();
    if let Some((unit, rest)) = text.split_once('(') {
        let n: i64 = rest.strip_suffix(')')?.trim().parse().ok()?;
        let (y, m, d) = civil_from_days(today);
        return Some(match unit.trim() {
            "DAYS_AGO" => today - n,
            "WEEKS_AGO" => today - 7 * n,
            "MONTHS_AGO" => {
                let months = y * 12 + i64::from(m) - 1 - n;
                days_from_civil(months.div_euclid(12), (months.rem_euclid(12) + 1) as u32, d)
            }
            "YEARS_AGO" => days_from_civil(y - n, m, d),
            _ => return None,
        });
    }
    let mut parts = text.split('-').map(str::parse::<u32>);
    let y = parts.next()?.ok()?;
    let m = parts.next()?.ok()?;
    let d = parts.next()?.ok()?;
    Some(days_from_civil(i64::from(y), m, d))
}

pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

struct Parser<'a> {
    s: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.s[self.pos..].chars().next()
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    fn error(&self, message: String) -> ParseError {
        ParseError {
            position: self.pos,
            message,
        }
    }

    fn term(&mut self) -> Result<Term, ParseError> {
        self.skip_ws();
        if self.peek() != Some('$') {
            return Err(self.error("expected $ATTRIBUTE".into()));
        }
        self.pos += 1;
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            self.pos += 1;
        }
        let token = &self.s[start..self.pos];
        let attribute = Attribute::parse(token)
            .ok_or_else(|| self.error(format!("unknown attribute ${token}")))?;
        self.skip_ws();
        let op = self.op()?;
        self.skip_ws();
        let value = self.value()?;
        Ok(Term {
            attribute,
            op,
            value,
        })
    }

    fn op(&mut self) -> Result<Op, ParseError> {
        let rest = &self.s[self.pos..];
        let table = [
            ("STARTS_WITH", Op::StartsWith),
            ("ENDS_WITH", Op::EndsWith),
            ("==", Op::Eq),
            ("<>", Op::Ne),
            ("<=", Op::Le),
            (">=", Op::Ge),
            ("!%", Op::NotContains),
            ("%", Op::Contains),
            ("~", Op::Similar),
            ("=", Op::Eq),
            ("<", Op::Lt),
            (">", Op::Gt),
        ];
        for (token, op) in table {
            if rest.starts_with(token) {
                self.pos += token.len();
                return Ok(op);
            }
        }
        Err(self.error("expected an operator".into()))
    }

    fn value(&mut self) -> Result<String, ParseError> {
        if self.peek() == Some('"') {
            self.pos += 1;
            let start = self.pos;
            while self.peek().is_some_and(|c| c != '"') {
                self.pos += self.peek().map_or(1, char::len_utf8);
            }
            if self.peek() != Some('"') {
                return Err(self.error("unterminated string".into()));
            }
            let value = self.s[start..self.pos].to_owned();
            self.pos += 1;
            Ok(value)
        } else {
            let start = self.pos;
            while self
                .peek()
                .is_some_and(|c| !c.is_whitespace() && c != '&' && c != '|')
            {
                self.pos += self.peek().map_or(1, char::len_utf8);
            }
            if start == self.pos {
                return Err(self.error("expected a value".into()));
            }
            Ok(self.s[start..self.pos].to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(comment: &str, label: &str, bpm: f64) -> TrackRow {
        TrackRow {
            comment: comment.into(),
            label: label.into(),
            bpm: Some(bpm),
            ..TrackRow::default()
        }
    }

    #[test]
    fn parses_and_evaluates_queries() {
        let q = Query::parse(r#"$COMMENT <> "" & $LABEL % "Drumcode""#).unwrap();
        assert!(q.matches(&track("4 deep", "Drumcode Ltd", 130.0)));
        assert!(!q.matches(&track("", "Drumcode", 130.0)));
        assert!(!q.matches(&track("x", "Other", 130.0)));

        let q = Query::parse(r#"$COMMENT STARTS_WITH "2""#).unwrap();
        assert!(q.matches(&track("2 warm", "", 0.0)));
        assert!(!q.matches(&track("12", "", 0.0)));
    }

    #[test]
    fn numbers_and_or() {
        let q = Query::parse("$BPM >= 135 | $COMMENT % tool").unwrap();
        assert!(q.matches(&track("", "", 136.0)));
        assert!(q.matches(&track("Tool A", "", 120.0)));
        assert!(!q.matches(&track("", "", 120.0)));
    }

    #[test]
    fn around_not_contains_and_all() {
        let q = Query::parse("$BPM ~ 128").unwrap();
        assert!(q.matches(&track("", "", 130.5)));
        assert!(!q.matches(&track("", "", 130.6)));
        let q = Query::parse(r#"$COMMENT !% "vocal""#).unwrap();
        assert!(q.matches(&track("dub", "", 0.0)));
        assert!(!q.matches(&track("Vocal mix", "", 0.0)));
        let q = Query::parse(r#"$ALL % "drum""#).unwrap();
        assert!(q.matches(&track("", "Drumcode", 0.0)));
    }

    #[test]
    fn ratings_are_stars() {
        let rated = TrackRow {
            rating: 204,
            ..TrackRow::default()
        };
        assert!(Query::parse("$RATING == 4").unwrap().matches(&rated));
        assert!(Query::parse("$RATING > 3").unwrap().matches(&rated));
    }

    #[test]
    fn keys_match_harmonically() {
        let am = TrackRow {
            musical_key: Some(21),
            ..TrackRow::default()
        };
        assert!(Query::parse("$KEY == 8A").unwrap().matches(&am));
        assert!(
            Query::parse("$KEY ~ C").unwrap().matches(&am),
            "relative major"
        );
        assert!(
            Query::parse("$KEY ~ Em").unwrap().matches(&am),
            "a fifth up"
        );
        assert!(!Query::parse("$KEY ~ D").unwrap().matches(&am));
    }

    #[test]
    fn dates_absolute_and_relative() {
        let today = days_from_civil(2025, 3, 20);
        let cx = Context {
            played: None,
            today,
        };
        let t = TrackRow {
            import_date: "2025/3/8".into(),
            ..TrackRow::default()
        };
        let q = |s: &str| Query::parse(s).unwrap().matches_in(&t, &cx);
        assert!(q("$IMPORTDATE >= DAYS_AGO(14)"), "in the last 14 days");
        assert!(!q("$IMPORTDATE >= WEEKS_AGO(1)"));
        assert!(q("$IMPORTDATE < 2025-03-09"));
        assert!(q("$IMPORTDATE == 2025-03-08"));
        assert!(q("$IMPORTDATE > MONTHS_AGO(1)"));
        assert_eq!(
            parse_date_value("YEARS_AGO(1)", today),
            Some(days_from_civil(2024, 3, 20))
        );
        assert_eq!(civil_from_days(today), (2025, 3, 20));
    }

    #[test]
    fn durations_booleans_and_played() {
        let t = TrackRow {
            id: 9,
            duration_s: Some(371.2),
            analyzed: true,
            ..TrackRow::default()
        };
        assert!(Query::parse("$TIME > 6:00").unwrap().matches(&t));
        assert!(Query::parse("$ANALYZED == TRUE").unwrap().matches(&t));
        assert!(
            Query::parse("$ANALYSISLOCKED == FALSE")
                .unwrap()
                .matches(&t)
        );
        let played = HashSet::from([9]);
        let cx = Context {
            played: Some(&played),
            today: 0,
        };
        let q = Query::parse("$PLAYED == TRUE").unwrap();
        assert!(q.matches_in(&t, &cx));
        assert!(!q.matches(&t));
    }

    #[test]
    fn prints_what_it_parses() {
        let text = r#"$GENRE % "Techno" & $BPM ~ 128 | $RATING >= 4"#;
        let q = Query::parse(text).unwrap();
        assert_eq!(q.to_string(), text);
        assert_eq!(Query::parse(&q.to_string()).unwrap(), q);
        assert_eq!(q.as_terms(), None, "mixed and/or is not one row list");
        let (terms, all) = Query::parse("$BPM > 120 | $BPM < 90")
            .unwrap()
            .as_terms()
            .unwrap();
        assert_eq!((terms.len(), all), (2, false));
        assert_eq!(
            Query::from_terms(terms, false).to_string(),
            "$BPM > 120 | $BPM < 90"
        );
    }

    #[test]
    fn search_field_syntax() {
        let t = TrackRow {
            artist: "Sonic Mayhem".into(),
            title: "Quake".into(),
            bpm: Some(140.0),
            ..TrackRow::default()
        };
        let s = |q: &str| Query::parse_search(q).unwrap().unwrap().matches(&t);
        assert!(s("sonic quake"), "space is and, any attribute");
        assert!(!s("sonic doom"));
        assert!(s(r#"$ARTIST == "Sonic Mayhem""#));
        assert!(s("doom | quake"));
        assert!(s("$BPM ~ 141 & mayhem"));
        assert!(s(r#""mayhem""#));
        assert_eq!(Query::parse_search("   ").unwrap(), None);
        assert!(Query::parse_search("$NOPE % x").is_err());
    }

    #[test]
    fn colours_by_name_or_number() {
        let red = TrackRow {
            color: 1,
            ..TrackRow::default()
        };
        assert!(Query::parse(r#"$COLOR == "Red""#).unwrap().matches(&red));
        assert!(Query::parse("$COLOR == 1").unwrap().matches(&red));
        assert!(Query::parse(r#"$COLOR <> "Blue""#).unwrap().matches(&red));
        assert!(!Query::parse(r#"$COLOR == "None""#).unwrap().matches(&red));
    }

    #[test]
    fn rejects_garbage() {
        assert!(Query::parse("COMMENT % x").is_err());
        assert!(Query::parse("$COMMENT ?? x").is_err());
        assert!(Query::parse(r#"$COMMENT % "open"#).is_err());
        assert!(Query::parse("$NOPE % x").is_err());
    }

    #[test]
    fn file_names_split_on_either_separator() {
        assert_eq!(file_name("/m/a b.mp3#2"), "a b.mp3");
        assert_eq!(file_name(r"C:\Music\a b.mp3"), "a b.mp3");
        assert_eq!(file_name("a.mp3"), "a.mp3");
    }
}
