use backspn_formats::nml::MusicalKey;
use backspn_library::smart::{self, Attribute, Kind, Op, Query, Term};
use eframe::egui::{self, ComboBox, RichText, Ui};

use crate::i18n::tr;
use crate::theme::WindowExt;
use crate::{search_combo, theme};

const OFFERED: [Attribute; 28] = [
    Attribute::All,
    Attribute::Color,
    Attribute::Artist,
    Attribute::Bitrate,
    Attribute::Bpm,
    Attribute::CatalogNo,
    Attribute::Comment,
    Attribute::Comment2,
    Attribute::FilePath,
    Attribute::FileName,
    Attribute::Genre,
    Attribute::Label,
    Attribute::Mix,
    Attribute::Producer,
    Attribute::Release,
    Attribute::Remixer,
    Attribute::Title,
    Attribute::Key,
    Attribute::KeyText,
    Attribute::PlayCount,
    Attribute::ImportDate,
    Attribute::ReleaseDate,
    Attribute::LastPlayedDate,
    Attribute::Rating,
    Attribute::Time,
    Attribute::Played,
    Attribute::Analyzed,
    Attribute::AnalysisLocked,
];

const UNITS: [(&str, &str); 4] = [
    ("days", "DAYS_AGO"),
    ("weeks", "WEEKS_AGO"),
    ("months", "MONTHS_AGO"),
    ("years", "YEARS_AGO"),
];

#[derive(Debug, Clone, PartialEq)]
struct Row {
    attribute: Attribute,
    choice: usize,
    value: String,
    unit: usize,
}

impl Row {
    fn new(attribute: Attribute) -> Self {
        let value = match attribute.kind() {
            Kind::Bool => "TRUE",
            Kind::Choice => "Red",
            Kind::Rating => "5",
            Kind::Key => "0",
            _ => "",
        };
        Self {
            attribute,
            choice: 0,
            value: value.to_owned(),
            unit: 0,
        }
    }

    fn choice(&self) -> smart::Choice {
        let all = smart::choices(self.attribute.kind());
        all[self.choice.min(all.len() - 1)]
    }

    fn from_term(term: &Term) -> Self {
        let kind = term.attribute.kind();
        let mut row = Self::new(term.attribute);
        let relative = kind == Kind::Date && term.value.contains('(');
        row.choice = smart::choices(kind)
            .iter()
            .position(|c| c.op == term.op && c.relative == relative)
            .unwrap_or(0);
        row.value = match kind {
            Kind::Date if relative => {
                let (unit, n) = term.value.split_once('(').unwrap_or(("DAYS_AGO", "1)"));
                row.unit = UNITS.iter().position(|u| u.1 == unit).unwrap_or(0);
                n.trim_end_matches(')').to_owned()
            }
            Kind::Date => term.value.replace('-', "/"),
            Kind::Key => MusicalKey::parse(&term.value)
                .map_or_else(|| term.value.clone(), |k| k.value().to_string()),
            _ => term.value.clone(),
        };
        row
    }

    fn term(&self) -> Result<Term, &'static str> {
        let choice = self.choice();
        let kind = self.attribute.kind();
        let v = self.value.trim();
        let value = match kind {
            Kind::Text => {
                let needs_value = matches!(
                    choice.op,
                    Op::Contains | Op::NotContains | Op::StartsWith | Op::EndsWith
                );
                if needs_value && v.is_empty() {
                    return Err(
                        "Please enter a value! Empty values are not supported for this operator.",
                    );
                }
                v.to_owned()
            }
            Kind::Tempo | Kind::Number | Kind::Rating => {
                v.parse::<f64>()
                    .map_err(|_| "Please enter a valid number!")?;
                v.to_owned()
            }
            Kind::Date if choice.relative => {
                let n: u32 = v.parse().map_err(|_| "Please enter a valid number!")?;
                format!("{}({n})", UNITS[self.unit].1)
            }
            Kind::Date => {
                let parts: Vec<u32> = v
                    .split('/')
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| INVALID_DATE)?;
                match parts.as_slice() {
                    [y, m, d] if (1..=12).contains(m) && (1..=31).contains(d) => {
                        format!("{y:04}-{m:02}-{d:02}")
                    }
                    _ => return Err(INVALID_DATE),
                }
            }
            Kind::Duration => {
                smart::parse_duration(v).ok_or(
                    "Please fill a valid time duration! The format should be mm:ss or h:mm:ss.",
                )?;
                v.to_owned()
            }
            Kind::Key => {
                let key = v.parse::<u8>().ok().and_then(MusicalKey::from_value);
                key.ok_or("Please select a key!")?.open_key()
            }
            Kind::Bool | Kind::Choice => v.to_owned(),
        };
        Ok(Term {
            attribute: self.attribute,
            op: choice.op,
            value,
        })
    }
}

const INVALID_DATE: &str = "Please fill a valid date! The format should be YYYY/MM/DD.";

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Save {
        node: Option<i64>,
        parent: Option<i64>,
        name: String,
        query: String,
    },
}

#[derive(Debug, Clone)]
pub struct SmartlistEditor {
    node: Option<i64>,
    parent: Option<i64>,
    name: String,
    all: bool,
    rows: Vec<Row>,
    raw: Option<String>,
    error: Option<String>,
}

impl SmartlistEditor {
    pub fn create(parent: Option<i64>, name: String) -> Self {
        Self {
            node: None,
            parent,
            name,
            all: true,
            rows: vec![Row::new(Attribute::Title)],
            raw: None,
            error: None,
        }
    }

    pub fn edit(node: i64, name: String, query: &str) -> Self {
        let parsed = Query::parse(query).ok();
        let (rows, all, raw) = match parsed.as_ref().and_then(Query::as_terms) {
            Some((terms, all)) => (terms.iter().map(Row::from_term).collect(), all, None),
            None => (Vec::new(), true, Some(query.to_owned())),
        };
        Self {
            node: Some(node),
            parent: None,
            name,
            all,
            rows,
            raw,
            error: None,
        }
    }

    fn query(&self) -> Result<String, String> {
        if let Some(raw) = &self.raw {
            return Query::parse(raw)
                .map(|_| raw.clone())
                .map_err(|e| e.to_string());
        }
        if self.rows.is_empty() {
            return Err("Add at least one condition.".to_owned());
        }
        let terms = self
            .rows
            .iter()
            .map(Row::term)
            .collect::<Result<Vec<_>, _>>()
            .map_err(str::to_owned)?;
        Ok(Query::from_terms(terms, self.all).to_string())
    }

    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        open: &mut bool,
        preview: impl Fn(&str) -> usize,
    ) -> Option<Outcome> {
        let mut outcome = None;
        let mut window_open = true;
        let title = if self.node.is_some() {
            "Edit Smartlist"
        } else {
            "New Smartlist"
        };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show_closable(ctx, &mut window_open, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("Name"));
                    ui.text_edit_singleline(&mut self.name);
                });
                ui.add_space(4.0);
                if let Some(raw) = &mut self.raw {
                    ui.label(tr("Query"));
                    ui.add(
                        egui::TextEdit::multiline(raw)
                            .desired_width(520.0)
                            .code_editor(),
                    );
                } else {
                    crate::widgets::check(ui, &mut self.all, "Match all conditions");
                    self.rows_ui(ui);
                }
                ui.separator();
                let query = self.query();
                match &query {
                    Ok(q) => {
                        let n = preview(q);
                        ui.label(RichText::new(format!("{n} matching tracks")).weak());
                    }
                    Err(e) => {
                        ui.colored_label(theme::get(ui.ctx()).danger, e);
                    }
                }
                if let Some(e) = &self.error {
                    ui.colored_label(theme::get(ui.ctx()).danger, e);
                }
                ui.horizontal(|ui| {
                    let ok = ui
                        .add_enabled(query.is_ok(), egui::Button::new("OK"))
                        .clicked();
                    if ui.button(tr("Cancel")).clicked() {
                        *open = false;
                    }
                    if ok && let Ok(query) = query {
                        if backspn_library::playlists::valid_name(&self.name) {
                            outcome = Some(Outcome::Save {
                                node: self.node,
                                parent: self.parent,
                                name: self.name.clone(),
                                query,
                            });
                        } else {
                            self.error = Some(
                                "Please enter a valid name! The name should not start with an '_' \
                                 or contain '$' and '\\'."
                                    .to_owned(),
                            );
                        }
                    }
                });
            });
        if !window_open {
            *open = false;
        }
        outcome
    }

    fn rows_ui(&mut self, ui: &mut Ui) {
        let mut remove = None;
        let mut add = None;
        let rows = self.rows.len();
        for (i, row) in self.rows.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let fields: Vec<_> = OFFERED
                    .iter()
                    .map(|&a| search_combo::Entry::item(a, a.name(), row.attribute == a))
                    .collect();
                if let (_, Some(a)) =
                    search_combo::show(ui, ("smart-attr", i), row.attribute.name(), 130.0, &fields)
                    && row.attribute != a
                {
                    *row = Row::new(a);
                }
                let choices = smart::choices(row.attribute.kind());
                ComboBox::from_id_salt(("smart-op", i))
                    .width(150.0)
                    .selected_text(row.choice().label)
                    .show_ui(ui, |ui| {
                        for (n, c) in choices.iter().enumerate() {
                            ui.selectable_value(&mut row.choice, n, c.label);
                        }
                    });
                value_ui(ui, i, row);
                if ui
                    .small_button("−")
                    .on_hover_text(tr("Remove condition"))
                    .clicked()
                    && rows > 1
                {
                    remove = Some(i);
                }
                if ui
                    .small_button("+")
                    .on_hover_text(tr("Add condition"))
                    .clicked()
                {
                    add = Some(i + 1);
                }
            });
        }
        if let Some(i) = remove {
            self.rows.remove(i);
        }
        if let Some(i) = add {
            self.rows.insert(i, Row::new(Attribute::Title));
        }
    }
}

fn value_ui(ui: &mut Ui, i: usize, row: &mut Row) {
    let text = |ui: &mut Ui, value: &mut String, hint: &str| {
        ui.add(
            egui::TextEdit::singleline(value)
                .hint_text(hint)
                .desired_width(160.0),
        );
    };
    match row.attribute.kind() {
        Kind::Bool => {
            ComboBox::from_id_salt(("smart-bool", i))
                .selected_text(if row.value == "FALSE" {
                    "False"
                } else {
                    "True"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut row.value, "TRUE".to_owned(), "True");
                    ui.selectable_value(&mut row.value, "FALSE".to_owned(), "False");
                });
        }
        Kind::Rating => {
            ComboBox::from_id_salt(("smart-rating", i))
                .selected_text(crate::columns::stars(
                    row.value.parse::<u8>().unwrap_or(0) * 51,
                ))
                .show_ui(ui, |ui| {
                    for n in 0..=5u8 {
                        ui.selectable_value(
                            &mut row.value,
                            n.to_string(),
                            crate::columns::stars(n * 51),
                        );
                    }
                });
        }
        Kind::Key => {
            let key = row
                .value
                .parse::<u8>()
                .ok()
                .and_then(MusicalKey::from_value);
            ComboBox::from_id_salt(("smart-key", i))
                .selected_text(key.map_or_else(String::new, |k| format!("{k} ({})", k.open_key())))
                .show_ui(ui, |ui| {
                    for v in 0..24u8 {
                        let k = MusicalKey::from_value(v).expect("in range");
                        ui.selectable_value(
                            &mut row.value,
                            v.to_string(),
                            format!("{k} ({})", k.open_key()),
                        );
                    }
                });
        }
        Kind::Date if row.choice().relative => {
            ui.add(egui::TextEdit::singleline(&mut row.value).desired_width(50.0));
            ComboBox::from_id_salt(("smart-unit", i))
                .width(80.0)
                .selected_text(UNITS[row.unit].0)
                .show_ui(ui, |ui| {
                    for (n, (name, _)) in UNITS.iter().enumerate() {
                        ui.selectable_value(&mut row.unit, n, *name);
                    }
                });
        }
        Kind::Choice => {
            ComboBox::from_id_salt(("smart-color", i))
                .selected_text(row.value.clone())
                .show_ui(ui, |ui| {
                    for name in backspn_library::smart::COLOR_NAMES {
                        ui.selectable_value(&mut row.value, name.to_owned(), name);
                    }
                });
        }
        Kind::Date => text(ui, &mut row.value, "YYYY/MM/DD"),
        Kind::Duration => text(ui, &mut row.value, "mm:ss"),
        _ => text(ui, &mut row.value, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_make_the_query_syntax() {
        let mut e = SmartlistEditor::create(None, "Peak".into());
        e.rows = vec![
            Row {
                value: "Techno".into(),
                ..Row::new(Attribute::Genre)
            },
            Row {
                value: "128".into(),
                ..Row::new(Attribute::Bpm)
            },
            Row {
                choice: 0,
                value: "14".into(),
                ..Row::new(Attribute::ImportDate)
            },
            Row {
                choice: 4,
                value: "2025/3/8".into(),
                ..Row::new(Attribute::LastPlayedDate)
            },
        ];
        assert_eq!(
            e.query().unwrap(),
            r#"$GENRE % "Techno" & $BPM ~ 128 & $IMPORTDATE >= DAYS_AGO(14) & $LASTPLAYEDDATE < 2025-03-08"#
        );
        e.all = false;
        assert!(e.query().unwrap().contains(" | "));
    }

    #[test]
    fn invalid_values_give_clear_messages() {
        let mut e = SmartlistEditor::create(None, "x".into());
        e.rows = vec![Row::new(Attribute::Title)];
        assert!(e.query().unwrap_err().starts_with("Please enter a value!"));
        e.rows = vec![Row {
            value: "fast".into(),
            ..Row::new(Attribute::Bpm)
        }];
        assert_eq!(e.query().unwrap_err(), "Please enter a valid number!");
        e.rows = vec![Row {
            choice: 2,
            value: "8 March".into(),
            ..Row::new(Attribute::ImportDate)
        }];
        assert_eq!(e.query().unwrap_err(), INVALID_DATE);
    }

    #[test]
    fn stored_queries_come_back_as_rows() {
        let text = r#"$COMMENT <> "" & $KEY ~ 1m & $IMPORTDATE < WEEKS_AGO(2)"#;
        let e = SmartlistEditor::edit(3, "Mine".into(), text);
        assert_eq!(e.rows.len(), 3);
        assert_eq!(e.rows[1].value, "21", "key as its value");
        assert_eq!((e.rows[2].unit, e.rows[2].value.as_str()), (1, "2"));
        assert!(e.rows[2].choice().relative);
        assert_eq!(e.query().unwrap(), text);
        let mixed = SmartlistEditor::edit(3, "Mixed".into(), "$BPM > 1 & $BPM < 2 | $BPM > 3");
        assert_eq!(
            mixed.query().unwrap(),
            "$BPM > 1 & $BPM < 2 | $BPM > 3",
            "kept as text"
        );
    }
}
