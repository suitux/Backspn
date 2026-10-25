use std::sync::mpsc::{self, Receiver};

use eframe::egui;
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{AboutMetadata, CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

use crate::theme::ThemeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCommand {
    Preferences,
    SetupWizard,
    ImportNml,
    ImportLibraryXml,
    ImportCollection,
    ExportCollection,
    ToggleBrowser,
    ToggleFx,
    Decks(usize),
    Theme(ThemeId),
    ReportProblem,
    ShowLogFile,
}

const PREFERENCES: &str = "preferences";
const SETUP_WIZARD: &str = "setup_wizard";
const QUIT: &str = "quit";
const IMPORT_NML: &str = "edit.import_nml";
const IMPORT_COLLECTION: &str = "edit.import_collection";
const IMPORT_LIBRARY_XML: &str = "edit.import_library_xml";
const EXPORT_COLLECTION: &str = "edit.export_collection";
const BROWSER: &str = "view.browser";
const FX: &str = "view.fx";
const TWO_DECKS: &str = "view.decks.2";
const FOUR_DECKS: &str = "view.decks.4";
const THEME_PREFIX: &str = "view.theme.";
const REPORT_PROBLEM: &str = "help.report_problem";
const SHOW_LOG: &str = "help.show_log";

fn theme_item_id(id: ThemeId) -> String {
    format!("{THEME_PREFIX}{}", id.name())
}

fn theme_from_item_id(item: &str) -> Option<ThemeId> {
    let name = item.strip_prefix(THEME_PREFIX)?;
    ThemeId::ALL.into_iter().find(|id| id.name() == name)
}

enum Titled {
    Item(MenuItem),
    Check(CheckMenuItem),
    Sub(Submenu),
    Predefined(PredefinedMenuItem),
}

impl Titled {
    fn set_text(&self, text: &str) {
        match self {
            Self::Item(i) => i.set_text(text),
            Self::Check(i) => i.set_text(text),
            Self::Sub(i) => i.set_text(text),
            Self::Predefined(i) => i.set_text(text),
        }
    }
}

pub struct AppMenu {
    _menu: Menu,
    titled: Vec<(Titled, &'static str)>,
    browser: CheckMenuItem,
    fx: CheckMenuItem,
    two_decks: CheckMenuItem,
    four_decks: CheckMenuItem,
    themes: Vec<(ThemeId, CheckMenuItem)>,
    events: Receiver<MenuCommand>,
}

impl AppMenu {
    #[allow(clippy::too_many_lines)]
    pub fn install(ctx: &egui::Context) -> Result<Self, muda::Error> {
        let cmd = |code| Some(Accelerator::new(Modifiers::META, code));
        let menu = Menu::new();

        let about = PredefinedMenuItem::about(
            None,
            Some(AboutMetadata {
                name: Some("Backspn".into()),
                version: Some(crate::support::version().into()),
                license: Some(env!("CARGO_PKG_LICENSE").into()),
                ..AboutMetadata::default()
            }),
        );
        let preferences = MenuItem::with_id(PREFERENCES, "Preferences…", true, cmd(Code::Comma));
        let setup_wizard = MenuItem::with_id(SETUP_WIZARD, "Setup Assistant…", true, None);
        let services = PredefinedMenuItem::services(None);
        let hide = PredefinedMenuItem::hide(None);
        let hide_others = PredefinedMenuItem::hide_others(None);
        let show_all = PredefinedMenuItem::show_all(None);
        let quit = MenuItem::with_id(QUIT, "Quit Backspn", true, cmd(Code::KeyQ));
        let app = Submenu::new("Backspn", true);
        app.append_items(&[
            &about,
            &PredefinedMenuItem::separator(),
            &preferences,
            &setup_wizard,
            &PredefinedMenuItem::separator(),
            &services,
            &PredefinedMenuItem::separator(),
            &hide,
            &hide_others,
            &show_all,
            &PredefinedMenuItem::separator(),
            &quit,
        ])?;

        let import_nml = MenuItem::with_id(IMPORT_NML, "Import NML…", true, None);
        let import_library_xml =
            MenuItem::with_id(IMPORT_LIBRARY_XML, "Import Library XML…", true, None);
        let import_collection =
            MenuItem::with_id(IMPORT_COLLECTION, "Import Collection…", true, None);
        let export_collection =
            MenuItem::with_id(EXPORT_COLLECTION, "Export Collection…", true, None);
        let edit = Submenu::new("Edit", true);
        edit.append_items(&[
            &import_nml,
            &import_library_xml,
            &import_collection,
            &export_collection,
        ])?;

        let browser = CheckMenuItem::with_id(BROWSER, "Browser", true, true, cmd(Code::KeyB));
        let fx = CheckMenuItem::with_id(FX, "FX Units", true, true, None);
        let two_decks = CheckMenuItem::with_id(TWO_DECKS, "2 Decks", true, true, None);
        let four_decks = CheckMenuItem::with_id(FOUR_DECKS, "4 Decks", true, false, None);
        let decks = Submenu::new("Decks", true);
        decks.append_items(&[&two_decks, &four_decks])?;
        let themes: Vec<(ThemeId, CheckMenuItem)> = ThemeId::ALL
            .into_iter()
            .map(|id| {
                let item = CheckMenuItem::with_id(theme_item_id(id), id.name(), true, false, None);
                (id, item)
            })
            .collect();
        let theme_menu = Submenu::new("Themes", true);
        for (_, item) in &themes {
            theme_menu.append(item)?;
        }
        let fullscreen = PredefinedMenuItem::fullscreen(None);
        let view = Submenu::new("View", true);
        view.append_items(&[
            &browser,
            &fx,
            &PredefinedMenuItem::separator(),
            &decks,
            &theme_menu,
            &PredefinedMenuItem::separator(),
            &fullscreen,
        ])?;

        let minimize = PredefinedMenuItem::minimize(None);
        let front = PredefinedMenuItem::bring_all_to_front(None);
        let window = Submenu::new("Window", true);
        window.append_items(&[&minimize, &PredefinedMenuItem::separator(), &front])?;

        let report_problem = MenuItem::with_id(REPORT_PROBLEM, "Report a Problem…", true, None);
        let show_log = MenuItem::with_id(SHOW_LOG, "Show Log File", true, None);
        let help = Submenu::new("Help", true);
        help.append_items(&[&report_problem, &show_log])?;
        let titled = vec![
            (Titled::Predefined(about), "About Backspn"),
            (Titled::Item(preferences), "Preferences…"),
            (Titled::Item(setup_wizard), "Setup Assistant…"),
            (Titled::Predefined(services), "Services"),
            (Titled::Predefined(hide), "Hide Backspn"),
            (Titled::Predefined(hide_others), "Hide Others"),
            (Titled::Predefined(show_all), "Show All"),
            (Titled::Item(quit), "Quit Backspn"),
            (Titled::Sub(edit.clone()), "Edit"),
            (Titled::Item(import_nml), "Import NML…"),
            (Titled::Item(import_library_xml), "Import Library XML…"),
            (Titled::Item(import_collection), "Import Collection…"),
            (Titled::Item(export_collection), "Export Collection…"),
            (Titled::Check(browser.clone()), "Browser"),
            (Titled::Check(fx.clone()), "FX Units"),
            (Titled::Check(two_decks.clone()), "2 Decks"),
            (Titled::Check(four_decks.clone()), "4 Decks"),
            (Titled::Sub(decks), "Decks"),
            (Titled::Sub(theme_menu), "Themes"),
            (Titled::Sub(view.clone()), "View"),
            (Titled::Predefined(fullscreen), "Toggle Full Screen"),
            (Titled::Sub(window.clone()), "Window"),
            (Titled::Predefined(minimize), "Minimize"),
            (Titled::Predefined(front), "Bring All to Front"),
            (Titled::Sub(help.clone()), "Help"),
            (Titled::Item(report_problem), "Report a Problem…"),
            (Titled::Item(show_log), "Show Log File"),
        ];

        menu.append_items(&[&app, &edit, &view, &window, &help])?;
        menu.init_for_nsapp();
        window.set_as_windows_menu_for_nsapp();
        help.set_as_help_menu_for_nsapp();

        let (tx, events) = mpsc::channel();
        let wake = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let command = match event.id().0.as_str() {
                PREFERENCES => MenuCommand::Preferences,
                SETUP_WIZARD => MenuCommand::SetupWizard,
                IMPORT_NML => MenuCommand::ImportNml,
                IMPORT_LIBRARY_XML => MenuCommand::ImportLibraryXml,
                IMPORT_COLLECTION => MenuCommand::ImportCollection,
                EXPORT_COLLECTION => MenuCommand::ExportCollection,
                BROWSER => MenuCommand::ToggleBrowser,
                FX => MenuCommand::ToggleFx,
                TWO_DECKS => MenuCommand::Decks(2),
                FOUR_DECKS => MenuCommand::Decks(4),
                REPORT_PROBLEM => MenuCommand::ReportProblem,
                SHOW_LOG => MenuCommand::ShowLogFile,
                QUIT => {
                    wake.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                other => match theme_from_item_id(other) {
                    Some(id) => MenuCommand::Theme(id),
                    None => return,
                },
            };
            tx.send(command).ok();
            wake.request_repaint();
        }));
        let menu = Self {
            _menu: menu,
            titled,
            browser,
            fx,
            two_decks,
            four_decks,
            themes,
            events,
        };
        menu.retitle();
        Ok(menu)
    }

    pub fn retitle(&self) {
        for (entry, english) in &self.titled {
            entry.set_text(crate::i18n::tr(english));
        }
    }

    pub fn commands(&self) -> Vec<MenuCommand> {
        self.events.try_iter().collect()
    }

    pub fn sync(&self, browser: bool, fx: bool, deck_count: usize, theme: ThemeId) {
        let checks = [
            (&self.browser, browser),
            (&self.fx, fx),
            (&self.two_decks, deck_ticks(deck_count)[0]),
            (&self.four_decks, deck_ticks(deck_count)[1]),
        ];
        let themes = self.themes.iter().map(|(id, item)| (item, *id == theme));
        for (item, on) in checks.into_iter().chain(themes) {
            if item.is_checked() != on {
                item.set_checked(on);
            }
        }
    }
}

fn deck_ticks(deck_count: usize) -> [bool; 2] {
    [deck_count != 4, deck_count == 4]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_deck_count_is_ticked() {
        assert_eq!(deck_ticks(2), [true, false]);
        assert_eq!(deck_ticks(4), [false, true]);
    }

    #[test]
    fn theme_item_ids_round_trip() {
        for id in ThemeId::ALL {
            assert_eq!(theme_from_item_id(&theme_item_id(id)), Some(id));
        }
        assert_eq!(theme_from_item_id("view.theme.Neon"), None);
        assert_eq!(theme_from_item_id("view.decks.2"), None);
    }
}
