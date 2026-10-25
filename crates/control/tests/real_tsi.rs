#[test]
#[ignore = "needs BACKSPN_TSI pointing to a copy of a real .tsi file"]
fn imports_real_settings() {
    let path = std::env::var("BACKSPN_TSI").expect("BACKSPN_TSI not set");
    let tsi = backspn_formats::tsi::Tsi::from_path(path).unwrap();
    for device in &tsi.devices {
        let r = backspn_control::tsi_import::import(device);
        eprintln!(
            "{}: {} imported, {} skipped",
            device.name,
            r.mappings.len(),
            r.skipped
        );
        for m in &r.mappings {
            eprintln!(
                "  {:<14} {:<26} {:?}{}",
                m.input.to_string(),
                m.action.label(),
                m.interaction,
                if m.invert { " (inverted)" } else { "" }
            );
        }
    }
}
