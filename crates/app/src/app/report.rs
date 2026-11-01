use std::time::Duration;

use eframe::egui::{Context, Ui, ViewportCommand};

use crate::App;
use crate::i18n::tr;
use crate::modal::{self, Ask};
use crate::support::update::{self, Stage, Target};
use crate::support::{crash::Pending, github, log};
use crate::toast;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CrashAnswer {
    Report,
    ShowLog,
}

const CRASH_TITLE: &str = "Crash Report";
const CRASH_TEXT: &str = "Backspn did not close properly last time. Report it on GitHub? \
     The issue opens in your browser with the error and the log; nothing is sent until you \
     send it.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateAnswer {
    Download,
    ReleasePage,
}

const UPDATE_TITLE: &str = "Update Backspn";
const UPDATE_TEXT: &str = "Download Backspn {v}? It installs when you restart or quit \
     Backspn, after checking that the download comes from Backspn.";
const RELEASE_PAGE: &str = "Opens the release's page in your browser";

fn update_ask(version: &str) -> Ask<UpdateAnswer> {
    Ask::new(UPDATE_TITLE, tr(UPDATE_TEXT).replace("{v}", version))
        .cancel("Later")
        .button("Download", UpdateAnswer::Download)
        .button("Release Notes", UpdateAnswer::ReleasePage)
}

fn crash_ask() -> Ask<CrashAnswer> {
    Ask::new(CRASH_TITLE, CRASH_TEXT)
        .cancel("Not Now")
        .button("Report on GitHub…", CrashAnswer::Report)
        .button("Show Log File", CrashAnswer::ShowLog)
}

impl App {
    pub(crate) fn offer_crash_report(&mut self, pending: Pending) {
        let crashed = pending.panic.is_some();
        self.count(|u| {
            if crashed {
                u.crashes += 1;
            } else {
                u.unclean_exits += 1;
            }
        });
        self.crash = Some(pending);
        self.crash_ask = Some(crash_ask());
    }

    pub(crate) fn crash_prompt(&mut self, ctx: &Context) {
        match modal::show(ctx, &mut self.crash_ask) {
            Some(CrashAnswer::Report) => {
                if let Some(p) = self.crash.take() {
                    github::open(&github::issue_url(&p.title(), &p.body()));
                }
            }
            Some(CrashAnswer::ShowLog) => {
                Self::show_log_file();
                self.crash_ask = Some(crash_ask());
            }
            None if self.crash_ask.is_none() => self.crash = None,
            None => {}
        }
    }

    pub(crate) fn report_problem() {
        let body = github::issue_body(
            "**What happened:**\n\n\n**What I expected:**\n",
            &log::this_session(),
        );
        github::open(&github::issue_url("", &body));
    }

    pub(crate) fn update_notice(&mut self, ui: &mut Ui) {
        if let Some(e) = self.update.poll() {
            toast::error(format!("{} {e}", tr("Cannot update Backspn:")));
        }
        let Some(release) = self.update.release.clone() else {
            return;
        };
        match self.update.stage() {
            Stage::Downloading(done) => {
                ui.label(
                    tr("Downloading Backspn {v}… {p} %")
                        .replace("{v}", release.version())
                        .replace("{p}", &format!("{:.0}", done * 100.0)),
                );
                if ui
                    .small_button("×")
                    .on_hover_text(tr("Cancel the download"))
                    .clicked()
                {
                    self.update.cancel();
                }
                ui.ctx().request_repaint_after(Duration::from_millis(250));
                return;
            }
            Stage::Ready => {
                if ui
                    .button(tr("Restart to Update"))
                    .on_hover_text(
                        tr("Installs Backspn {v} and starts it again. Quitting also installs it.")
                            .replace("{v}", release.version()),
                    )
                    .clicked()
                {
                    update::restart_after_quit(true);
                    ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                }
                return;
            }
            Stage::Idle => {}
        }
        if !update::shows(&release, self.settings.dismissed_version.as_deref()) {
            return;
        }
        let text = tr("Backspn {v} is available").replace("{v}", release.version());
        let hover = match &self.update.target {
            Target::Unsupported(why) if !why.is_empty() => {
                format!("{}\n{}", tr(RELEASE_PAGE), tr(why))
            }
            Target::Unsupported(_) => tr(RELEASE_PAGE).to_owned(),
            _ => tr("Download and install the new version").to_owned(),
        };
        if ui.link(text).on_hover_text(hover).clicked() {
            if self.update.can_install() {
                self.update_ask = Some(update_ask(release.version()));
            } else {
                github::open(&release.url());
            }
        }
        if ui
            .small_button("×")
            .on_hover_text(tr("Hide until the next version"))
            .clicked()
        {
            self.settings.dismissed_version = Some(release.version().to_owned());
        }
    }

    pub(crate) fn update_prompt(&mut self, ctx: &Context) {
        match modal::show(ctx, &mut self.update_ask) {
            Some(UpdateAnswer::Download) => self.update.download(),
            Some(UpdateAnswer::ReleasePage) => {
                if let Some(release) = &self.update.release {
                    github::open(&release.url());
                }
            }
            None => {}
        }
    }

    pub(crate) fn show_log_file() {
        crate::browser::reveal(&log::path().to_string_lossy());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crash_report_waits_for_an_answer() {
        let (mut app, _renderer) = App::offline();
        app.offer_crash_report(Pending {
            panic: None,
            log: "x - boom\n".into(),
        });
        assert!(app.crash.is_some() && app.crash_ask.is_some());
    }
}
