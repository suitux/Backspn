pub mod bstem;
pub mod dj_playlists;
pub mod interop;
pub mod music_library;
pub mod nml;
pub mod stem;
pub mod transients;
pub mod tsi;
pub mod xml;

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("XML error at byte {position}: {source}")]
    Xml {
        position: u64,
        #[source]
        source: quick_xml::Error,
    },
    #[error("malformed XML at byte {position}: {message}")]
    Malformed { position: u64, message: String },
    #[error("invalid <{element}>: {message}")]
    Invalid { element: String, message: String },
}
