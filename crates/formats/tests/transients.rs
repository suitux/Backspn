use std::path::Path;

use backspn_formats::transients;

#[test]
#[ignore = "needs BACKSPN_TRANSIENTS pointing to a Transients directory"]
fn reads_real_transient_cache() {
    let dir = std::env::var("BACKSPN_TRANSIENTS").expect("BACKSPN_TRANSIENTS not set");
    let mut files = 0;
    let mut onsets = 0;
    visit(Path::new(&dir), &mut |path| {
        let t = transients::from_path(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            t.windows(2).all(|w| w[0].position_s <= w[1].position_s),
            "{}: positions not sorted",
            path.display()
        );
        files += 1;
        onsets += t.len();
    });
    assert!(files > 0, "no files under {dir}");
    eprintln!("{files} files, {onsets} transients");
}

fn visit(dir: &Path, f: &mut impl FnMut(&Path)) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            visit(&path, f);
        } else if path
            .file_name()
            .is_some_and(|n| !n.to_string_lossy().starts_with('.'))
        {
            f(&path);
        }
    }
}
