use backspn_library::Library;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["import", nml, db] => {
            let report = Library::open(db)?.import_nml(nml)?;
            println!("{report:?}");
        }
        ["stats", db] => {
            let lib = Library::open(db)?;
            let tracks = lib.tracks()?;
            let analysed = tracks.iter().filter(|t| t.analyzed).count();
            println!(
                "{} tracks ({analysed} with a grid), {} playlist nodes",
                tracks.len(),
                lib.nodes()?.len()
            );
        }
        _ => {
            eprintln!("usage: nml import NML DB | nml stats DB");
            std::process::exit(2);
        }
    }
    Ok(())
}
