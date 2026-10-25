use super::{Location, invalid, num, owned, required};
use crate::Error;
use crate::xml::Element;

pub const SLOTS: usize = 4;
pub const CELLS: usize = 16;

#[derive(Debug, Clone, PartialEq)]
pub struct SampleSet {
    pub title: String,
    pub quant_index: u32,
    pub quant_on: bool,
    pub location: Option<Location>,
    pub author_type: Option<String>,
    pub import_date: Option<String>,
    pub bpm: Option<f64>,
    pub slots: Vec<SetSlot>,
}

impl SampleSet {
    pub fn entry_count(&self) -> usize {
        let keys: std::collections::HashSet<String> = self
            .slots
            .iter()
            .flat_map(|s| &s.cells)
            .map(|c| c.location.primary_key())
            .collect();
        keys.len()
    }
}

impl Default for SampleSet {
    fn default() -> Self {
        Self {
            title: "New Sample Set".to_owned(),
            quant_index: 2,
            quant_on: true,
            location: None,
            author_type: None,
            import_date: None,
            bpm: None,
            slots: (0..SLOTS).map(|_| SetSlot::default()).collect(),
        }
    }
}

impl SampleSet {
    pub fn from_element(e: &Element) -> Result<Self, Error> {
        Ok(Self {
            title: owned(e, "TITLE").unwrap_or_default(),
            quant_index: num(e, "QUANT_VAlUE")?.unwrap_or(0),
            quant_on: e.attr("QUANT_STATE") == Some("1"),
            location: e
                .child("LOCATION")
                .map(Location::from_element)
                .transpose()?,
            author_type: e
                .child("MODIFICATION_INFO")
                .and_then(|m| owned(m, "AUTHOR_TYPE")),
            import_date: e.child("INFO").and_then(|i| owned(i, "IMPORT_DATE")),
            bpm: e
                .child("TEMPO")
                .map(|t| num(t, "BPM"))
                .transpose()?
                .flatten(),
            slots: e
                .children_named("SLOT")
                .map(SetSlot::from_element)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetSlot {
    pub keylock: bool,
    pub fx_enable: bool,
    pub punch: bool,
    pub active_cell: u32,
    pub cells: Vec<SetCell>,
}

impl Default for SetSlot {
    fn default() -> Self {
        Self {
            keylock: false,
            fx_enable: true,
            punch: false,
            active_cell: 0,
            cells: Vec::new(),
        }
    }
}

impl SetSlot {
    fn from_element(e: &Element) -> Result<Self, Error> {
        Ok(Self {
            keylock: e.attr("KEYLOCK") == Some("1"),
            fx_enable: e.attr("FXENABLE") == Some("1"),
            punch: e.attr("PUNCHMODE") == Some("1"),
            active_cell: num(e, "ACTIVE_CELL_INDEX")?.unwrap_or(0),
            cells: e
                .children_named("CELL")
                .map(SetCell::from_element)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetCell {
    pub index: u32,
    pub name: String,
    pub color: u32,
    pub sync: bool,
    pub reverse: bool,
    pub trigger_type: u32,
    pub play_type: u32,
    pub speed: f64,
    pub transpose: f64,
    pub offset: f64,
    pub nudge: Option<f64>,
    pub gain: f64,
    pub start_marker: f64,
    pub end_marker: f64,
    pub bpm: f64,
    pub location: Location,
}

impl SetCell {
    pub fn new(index: u32, location: Location) -> Self {
        Self {
            index,
            name: String::new(),
            color: 0,
            sync: true,
            reverse: false,
            trigger_type: 1,
            play_type: 0,
            speed: 1.0,
            transpose: 0.0,
            offset: 0.0,
            nudge: Some(0.0),
            gain: 0.5,
            start_marker: 0.0,
            end_marker: 0.0,
            bpm: 0.0,
            location,
        }
    }

    fn from_element(e: &Element) -> Result<Self, Error> {
        let req = |key: &str| -> Result<f64, Error> {
            num(e, key)?.ok_or_else(|| invalid(e, format!("missing attribute {key}")))
        };
        let location = Location {
            dir: required(e, "DIR")?.to_owned(),
            file: required(e, "FILE")?.to_owned(),
            volume: required(e, "VOLUME")?.to_owned(),
            volume_id: None,
        };
        Ok(Self {
            index: num(e, "INDEX")?.unwrap_or(0),
            name: owned(e, "CELLNAME").unwrap_or_default(),
            color: num(e, "COLOR")?.unwrap_or(0),
            sync: e.attr("SYNC") == Some("1"),
            reverse: e.attr("REVERSE") == Some("1"),
            trigger_type: num(e, "MODE")?.unwrap_or(1),
            play_type: num(e, "TYPE")?.unwrap_or(0),
            speed: req("SPEED")?,
            transpose: req("TRANSPOSE")?,
            offset: req("OFFSET")?,
            nudge: num(e, "NUDGE")?,
            gain: req("GAIN")?,
            start_marker: req("START_MARKER")?,
            end_marker: req("END_MARKER")?,
            bpm: req("BPM")?,
            location,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml;

    const SET: &str = r#"<SET TITLE="New SampleDeck Set" QUANT_VAlUE="1" QUANT_STATE="1"><LOCATION DIR="/:Users/:x/:" FILE="2026y04m25d_14h26m56s156576613456750.set" VOLUME="Macintosh HD" VOLUMEID="Macintosh HD"></LOCATION>
<MODIFICATION_INFO AUTHOR_TYPE="importer"></MODIFICATION_INFO>
<INFO IMPORT_DATE="2026/4/25"></INFO>
<TEMPO BPM="140.684814"></TEMPO>
<SLOT KEYLOCK="1" FXENABLE="1" PUNCHMODE="0" ACTIVE_CELL_INDEX="0"><CELL INDEX="0" CELLNAME="BT7AAD0" COLOR="10" SYNC="1" REVERSE="0" MODE="1" TYPE="1" SPEED="1.000000" TRANSPOSE="0.011765" OFFSET="0.000000" NUDGE="0.000000" GAIN="0.729412" START_MARKER="0.000000" END_MARKER="0.000000" BPM="140.689850" DIR="/:Samples/:Kit A/:" FILE="BT7AAD0.WAV" VOLUME="USB"></CELL>
</SLOT>
<SLOT KEYLOCK="1" FXENABLE="1" PUNCHMODE="0" ACTIVE_CELL_INDEX="0"></SLOT>
<SLOT KEYLOCK="1" FXENABLE="1" PUNCHMODE="0" ACTIVE_CELL_INDEX="0"></SLOT>
<SLOT KEYLOCK="1" FXENABLE="1" PUNCHMODE="0" ACTIVE_CELL_INDEX="0"></SLOT>
</SET>
"#;

    #[test]
    fn a_set_is_read() {
        let doc = xml::parse(SET).unwrap();
        let set = SampleSet::from_element(&doc.root).unwrap();
        assert_eq!(set.title, "New SampleDeck Set");
        assert_eq!((set.quant_index, set.quant_on), (1, true));
        assert_eq!(set.bpm, Some(140.684_814));
        assert_eq!(set.slots.len(), 4);
        assert_eq!(set.entry_count(), 1, "distinct samples");
        let cell = &set.slots[0].cells[0];
        assert_eq!(cell.name, "BT7AAD0");
        assert_eq!((cell.trigger_type, cell.play_type), (1, 1));
        assert_eq!(cell.location.file, "BT7AAD0.WAV");
        assert!(set.slots[0].keylock && !set.slots[0].punch);
    }

    #[test]
    fn a_cell_without_a_required_attribute_is_an_error() {
        let doc = xml::parse(
            r#"<CELL INDEX="0" CELLNAME="x" COLOR="0" SYNC="1" REVERSE="0" MODE="1" TYPE="0" SPEED="1.0" DIR="/:a/:" FILE="b" VOLUME="c"></CELL>"#,
        )
        .unwrap();
        assert!(SetCell::from_element(&doc.root).is_err());
    }

    #[test]
    fn new_cells_take_the_defaults() {
        let cell = SetCell::new(3, Location::default());
        assert!(cell.sync && !cell.reverse);
        assert_eq!((cell.trigger_type, cell.play_type), (1, 0));
        assert!((cell.gain - 0.5).abs() < 1e-9 && (cell.speed - 1.0).abs() < 1e-9);
        let slot = SetSlot::default();
        assert!(!slot.keylock && slot.fx_enable && !slot.punch);
    }
}
