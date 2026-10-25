use eframe::egui::{self, ColorImage, IconData, TextureHandle, TextureOptions};
use image::RgbaImage;

const PNG: &[u8] = include_bytes!("../icon/backspn.png");

const LOGO_PX: u32 = 96;

fn decode() -> Option<RgbaImage> {
    Some(image::load_from_memory(PNG).ok()?.into_rgba8())
}

pub fn app_icon() -> Option<IconData> {
    let image = decode()?;
    let (width, height) = image.dimensions();
    Some(IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

pub fn logo(ctx: &egui::Context) -> Option<TextureHandle> {
    let id = egui::Id::unique("backspn-logo");
    if let Some(texture) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return Some(texture);
    }
    let small = image::imageops::resize(
        &decode()?,
        LOGO_PX,
        LOGO_PX,
        image::imageops::FilterType::Lanczos3,
    );
    let color =
        ColorImage::from_rgba_unmultiplied([LOGO_PX as usize, LOGO_PX as usize], small.as_raw());
    let texture = ctx.load_texture("backspn-logo", color, TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    Some(texture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_icon_decodes_square_with_transparency() {
        let icon = app_icon().expect("icon decodes");
        assert_eq!((icon.width, icon.height), (512, 512));
        assert_eq!(icon.rgba.len(), 512 * 512 * 4);
        let alphas = icon.rgba.iter().skip(3).step_by(4);
        assert!(alphas.clone().any(|&a| a == 0), "transparent corners");
        assert!(alphas.clone().any(|&a| a == 255), "opaque body");
    }

    #[test]
    fn logo_texture_is_loaded_once_per_context() {
        let ctx = egui::Context::default();
        let first = logo(&ctx).expect("logo loads");
        assert_eq!(first.size(), [LOGO_PX as usize; 2]);
        let again = logo(&ctx).expect("logo loads");
        assert_eq!(first.id(), again.id());
    }
}
