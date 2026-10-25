use backspn_control::{MappingSet, tsi_import};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: tsi2json SETTINGS.tsi OUT.json");
        std::process::exit(2);
    };
    let tsi = backspn_formats::tsi::Tsi::from_path(&input)?;
    let mut set = MappingSet::default();
    for device in &tsi.devices {
        let result = tsi_import::import(device);
        println!(
            "{}: {} imported, {} skipped",
            device.name,
            result.mappings.len(),
            result.skipped
        );
        set.mappings.extend(result.mappings);
    }
    set.save(std::path::Path::new(&output))?;
    Ok(())
}
