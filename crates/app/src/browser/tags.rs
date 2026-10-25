use std::path::Path;

use backspn_formats::nml::KeyNotation;

use super::Browser;
use super::files::is_inside_other_app_data;

impl Browser {
    pub fn set_tag_writing(&mut self, write: bool, notation: KeyNotation) {
        self.tag_writer.after_changes = write;
        self.tag_writer.notation = notation;
    }

    pub fn tags_changed(&mut self, tracks: &[i64]) {
        if self.tag_writer.after_changes {
            self.export_tags(tracks);
        }
    }

    pub(super) fn export_tags(&mut self, tracks: &[i64]) {
        for &id in tracks {
            if let Some(track) = self
                .row(id)
                .filter(|t| t.id > 0 && !is_inside_other_app_data(Path::new(&t.path)))
                .cloned()
            {
                self.tag_writer.schedule(&track);
            }
        }
    }

    pub(super) fn collect_tag_writes(&mut self) {
        for (id, result) in self.tag_writer.poll() {
            match result {
                Ok(()) => {
                    let result = self.with_lib(|lib| lib.update_from_file(id));
                    Self::report(result);
                    self.refresh_track(id);
                }
                Err(e) => {
                    log::warn!("{e}");
                    crate::toast::error(e);
                }
            }
        }
        if self.tag_writer.busy()
            && let Some(ctx) = &self.ctx
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}
