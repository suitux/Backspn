use eframe::egui::{Key, Modifiers};

pub fn name(key: Key, modifiers: Modifiers) -> String {
    let base = match key {
        Key::ArrowUp => "Up",
        Key::ArrowDown => "Down",
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",
        Key::Quote => "'",
        Key::Minus => "-",
        Key::Space => "Space",
        k => k.symbol_or_name(),
    };
    let mut out = String::new();
    if modifiers.shift {
        out.push_str("Shift+");
    }
    if modifiers.ctrl || modifiers.mac_cmd {
        out.push_str("Ctrl+");
    }
    if modifiers.alt {
        out.push_str("Alt+");
    }
    out.push_str(base);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_the_mappings() {
        assert_eq!(name(Key::T, Modifiers::NONE), "T");
        assert_eq!(name(Key::T, Modifiers::SHIFT), "Shift+T");
        assert_eq!(name(Key::Num1, Modifiers::CTRL), "Ctrl+1");
        assert_eq!(name(Key::OpenBracket, Modifiers::SHIFT), "Shift+[");
        assert_eq!(name(Key::Quote, Modifiers::NONE), "'");
        assert_eq!(name(Key::ArrowLeft, Modifiers::SHIFT), "Shift+Left");
    }
}
