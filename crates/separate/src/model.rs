use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};

use crate::Error;

pub const MODEL_URL: &str =
    "https://huggingface.co/StemSplitio/htdemucs-onnx/resolve/main/htdemucs_fp16weights.onnx";
pub const MODEL_FILE: &str = "htdemucs_fp16weights.onnx";
pub const MODEL_SHA256: &str = "d05c269d0178d2a72ad484b10b11dd370193fc923201c3b27a99f848745db70a";
pub const MODEL_SIZE: u64 = 165_612_636;

fn sha256(mut reader: impl Read) -> std::io::Result<String> {
    let mut hash = Sha256::new();
    let mut buf = vec![0; 1 << 16];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(hash.finalize().iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    }))
}

pub fn verify(path: &Path) -> bool {
    std::fs::File::open(path)
        .and_then(sha256)
        .is_ok_and(|h| h == MODEL_SHA256)
}

pub fn download(
    path: &Path,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<(), Error> {
    let partial = path.with_extension("part");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let response = ureq::get(MODEL_URL)
        .call()
        .map_err(|e| Error::Download(e.to_string()))?;
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok()?.parse().ok())
        .unwrap_or(MODEL_SIZE);
    let mut body = response.into_body().into_reader();
    let mut file = std::fs::File::create(&partial)?;
    let mut buf = vec![0; 1 << 16];
    let mut got = 0;
    let result = loop {
        if cancel.load(Ordering::Relaxed) {
            break Err(Error::Cancelled);
        }
        match body.read(&mut buf) {
            Ok(0) => break Ok(()),
            Ok(n) => {
                file.write_all(&buf[..n])?;
                got += n as u64;
                progress(got, total);
            }
            Err(e) => break Err(Error::Download(e.to_string())),
        }
    };
    drop(file);
    if let Err(e) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(e);
    }
    if !verify(&partial) {
        let _ = std::fs::remove_file(&partial);
        return Err(Error::Checksum);
    }
    std::fs::rename(&partial, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_refuses_other_files() {
        assert_eq!(
            sha256(&b"abc"[..]).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let dir = std::env::temp_dir().join(format!("backspn-model-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join(MODEL_FILE);
        std::fs::write(&fake, b"not a model").unwrap();
        assert!(!verify(&fake));
        assert!(!verify(&dir.join("missing.onnx")));
        let _ = std::fs::remove_dir_all(dir);
    }
}
