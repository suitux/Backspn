use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use backspn_formats::nml::{Location, SET_CELLS, SampleSet, SetCell, SetSlot};
use backspn_library::clock::LocalTime;
use serde::{Deserialize, Serialize};

pub const SET_FILE: &str = "set.json";

const FORMAT: &str = "backspn.sample-set";
const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct SetFile {
    format: String,
    version: u32,
    title: String,
    bpm: Option<f64>,
    quantize: u32,
    quantize_on: bool,
    slots: Vec<SlotFile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SlotFile {
    keylock: bool,
    fx_enable: bool,
    punch: bool,
    active_cell: u32,
    cells: Vec<CellFile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CellFile {
    index: u32,
    name: String,
    color: u32,
    sync: bool,
    reverse: bool,
    trigger_type: u32,
    play_type: u32,
    speed: f64,
    transpose: f64,
    offset: f64,
    nudge: Option<f64>,
    gain: f64,
    start_marker: f64,
    end_marker: f64,
    bpm: f64,
    file: String,
}

impl SetFile {
    fn from_set(set: &SampleSet) -> Self {
        Self {
            format: FORMAT.to_owned(),
            version: VERSION,
            title: set.title.clone(),
            bpm: set.bpm,
            quantize: set.quant_index,
            quantize_on: set.quant_on,
            slots: set
                .slots
                .iter()
                .map(|s| SlotFile {
                    keylock: s.keylock,
                    fx_enable: s.fx_enable,
                    punch: s.punch,
                    active_cell: s.active_cell,
                    cells: s.cells.iter().map(CellFile::from_cell).collect(),
                })
                .collect(),
        }
    }

    fn into_set(self) -> SampleSet {
        SampleSet {
            title: self.title,
            quant_index: self.quantize,
            quant_on: self.quantize_on,
            bpm: self.bpm,
            slots: self
                .slots
                .into_iter()
                .map(|s| SetSlot {
                    keylock: s.keylock,
                    fx_enable: s.fx_enable,
                    punch: s.punch,
                    active_cell: s.active_cell,
                    cells: s.cells.into_iter().map(CellFile::into_cell).collect(),
                })
                .collect(),
            ..SampleSet::default()
        }
    }
}

impl CellFile {
    fn from_cell(c: &SetCell) -> Self {
        Self {
            index: c.index,
            name: c.name.clone(),
            color: c.color,
            sync: c.sync,
            reverse: c.reverse,
            trigger_type: c.trigger_type,
            play_type: c.play_type,
            speed: c.speed,
            transpose: c.transpose,
            offset: c.offset,
            nudge: c.nudge,
            gain: c.gain,
            start_marker: c.start_marker,
            end_marker: c.end_marker,
            bpm: c.bpm,
            file: c.location.file.clone(),
        }
    }

    fn into_cell(self) -> SetCell {
        let location = Location {
            dir: String::new(),
            file: self.file,
            volume: String::new(),
            volume_id: None,
        };
        SetCell {
            index: self.index,
            name: self.name,
            color: self.color,
            sync: self.sync,
            reverse: self.reverse,
            trigger_type: self.trigger_type,
            play_type: self.play_type,
            speed: self.speed,
            transpose: self.transpose,
            offset: self.offset,
            nudge: self.nudge,
            gain: self.gain,
            start_marker: self.start_marker,
            end_marker: self.end_marker,
            bpm: self.bpm,
            location,
        }
    }
}

pub fn folder_name(title: &str) -> String {
    let name: String = title
        .chars()
        .map(|c| if matches!(c, ':' | '/') { '-' } else { c })
        .collect();
    let name = name.trim();
    if name.is_empty() {
        "Sample Set".to_owned()
    } else {
        name.to_owned()
    }
}

pub fn free_folder(parent: &Path, name: &str, now: LocalTime) -> PathBuf {
    let plain = parent.join(name);
    if !plain.exists() {
        return plain;
    }
    let stamp = crate::sample_capture::name("", now);
    parent.join(format!("{name}_{}", stamp.trim()))
}

pub fn export(
    set: &SampleSet,
    files: &[Option<PathBuf>],
    parent: &Path,
    now: LocalTime,
) -> io::Result<PathBuf> {
    let folder = free_folder(parent, &folder_name(&set.title), now);
    std::fs::create_dir_all(&folder)?;
    let mut copied: HashMap<PathBuf, String> = HashMap::new();
    let mut out = set.clone();
    for (slot, s) in out.slots.iter_mut().enumerate() {
        let mut kept = Vec::new();
        for mut cell in std::mem::take(&mut s.cells) {
            let Some(Some(source)) = files.get(slot * SET_CELLS + cell.index as usize) else {
                continue;
            };
            let name = if let Some(name) = copied.get(source) {
                name.clone()
            } else {
                let name = unique_name(&folder, source);
                std::fs::copy(source, folder.join(&name))?;
                copied.insert(source.clone(), name.clone());
                name
            };
            cell.location = Location {
                dir: String::new(),
                file: name,
                volume: String::new(),
                volume_id: None,
            };
            kept.push(cell);
        }
        s.cells = kept;
    }
    let text = serde_json::to_string_pretty(&SetFile::from_set(&out)).map_err(io::Error::other)?;
    std::fs::write(folder.join(SET_FILE), text)?;
    Ok(folder)
}

fn unique_name(folder: &Path, source: &Path) -> String {
    let file = source
        .file_name()
        .map_or_else(|| "sample".to_owned(), |f| f.to_string_lossy().into_owned());
    if !folder.join(&file).exists() {
        return file;
    }
    let stem = source
        .file_stem()
        .map_or_else(|| "sample".to_owned(), |f| f.to_string_lossy().into_owned());
    let ext = source
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (2..10_000)
        .map(|n| format!("{stem} {n}{ext}"))
        .find(|f| !folder.join(f).exists())
        .unwrap_or(file)
}

pub fn read(folder: &Path) -> Result<(SampleSet, Vec<Option<PathBuf>>), String> {
    let text = std::fs::read_to_string(folder.join(SET_FILE))
        .map_err(|e| format!("{}: {e}", folder.join(SET_FILE).display()))?;
    let file: SetFile = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if file.format != FORMAT {
        return Err(format!(
            "{}: not a sample set",
            folder.join(SET_FILE).display()
        ));
    }
    let set = file.into_set();
    let mut files = vec![None; set.slots.len() * SET_CELLS];
    for (slot, s) in set.slots.iter().enumerate() {
        for cell in &s.cells {
            let path = folder.join(&cell.location.file);
            if path.is_file()
                && let Some(f) = files.get_mut(slot * SET_CELLS + cell.index as usize)
            {
                *f = Some(path);
            }
        }
    }
    Ok((set, files))
}

pub fn copy_in(folder: &Path, parent: &Path, title: &str, now: LocalTime) -> io::Result<PathBuf> {
    let target = free_folder(parent, &folder_name(title), now);
    std::fs::create_dir_all(&target)?;
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), target.join(entry.file_name()))?;
        }
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(n: u32) -> LocalTime {
        LocalTime {
            year: 2026,
            month: 10,
            day: 22,
            seconds: n,
        }
    }

    #[test]
    fn titles_make_folder_names() {
        assert_eq!(folder_name("Drums: 1/2"), "Drums- 1-2");
        assert_eq!(folder_name("  "), "Sample Set");
    }

    #[test]
    fn a_set_goes_out_to_a_folder_and_back() {
        let dir = std::env::temp_dir().join(format!("backspn-setfolder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let src = dir.join("src");
        std::fs::create_dir_all(src.join("other")).unwrap();
        for f in ["kick.wav", "other/kick.wav"] {
            std::fs::write(src.join(f), f).unwrap();
        }
        let none = Location {
            dir: String::new(),
            file: String::new(),
            volume: String::new(),
            volume_id: None,
        };
        let mut set = SampleSet {
            title: "Kit/A".to_owned(),
            ..SampleSet::default()
        };
        let mut a = SetCell::new(0, none.clone());
        a.name = "Kick".to_owned();
        set.slots[0].cells = vec![a, SetCell::new(1, none.clone()), SetCell::new(2, none)];
        let mut files = vec![None; 64];
        files[0] = Some(src.join("kick.wav"));
        files[1] = Some(src.join("other/kick.wav"));
        let out = dir.join("out");
        let folder = export(&set, &files, &out, at(0)).unwrap();
        assert_eq!(folder, out.join("Kit-A"));
        assert_eq!(
            std::fs::read_to_string(folder.join("kick 2.wav")).unwrap(),
            "other/kick.wav",
            "a clashing name gets a number"
        );
        let again = export(&set, &files, &out, at(3661)).unwrap();
        assert_eq!(again, out.join("Kit-A_2026-10-22 01-01-01"));

        let text = std::fs::read_to_string(folder.join(SET_FILE)).unwrap();
        assert!(
            text.contains("\"format\": \"backspn.sample-set\""),
            "{text}"
        );
        let (back, paths) = read(&folder).unwrap();
        assert_eq!(back.title, "Kit/A");
        assert_eq!(
            back.slots[0].cells.len(),
            2,
            "the cell without a sample is left out"
        );
        assert_eq!(back.slots[0].cells[0].name, "Kick");
        let (sent, got) = (&set.slots[0].cells[0], &back.slots[0].cells[0]);
        assert_eq!(
            (got.gain, got.sync, got.nudge),
            (sent.gain, sent.sync, sent.nudge)
        );
        assert_eq!(got.location.file, "kick.wav");
        assert_eq!(paths[0], Some(folder.join("kick.wav")));
        assert_eq!(paths[1], Some(folder.join("kick 2.wav")));
        assert_eq!(paths[2], None);

        let sets = dir.join("sets");
        let kept = copy_in(&folder, &sets, &back.title, at(0)).unwrap();
        assert!(kept.join(SET_FILE).is_file() && kept.join("kick.wav").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
