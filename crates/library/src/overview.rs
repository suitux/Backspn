//! Layout of the waveform overview stored with each track: `COLUMNS`
//! columns of `STRIDE` bytes, the first byte of each being the amplitude.

pub const COLUMNS: usize = 400;

pub const STRIDE: usize = 7;
pub const OLD_STRIDE: usize = 4;

pub fn stride(overview: &[u8]) -> usize {
    if overview.len() == COLUMNS * OLD_STRIDE {
        OLD_STRIDE
    } else {
        STRIDE
    }
}

pub fn amplitudes(overview: &[u8]) -> impl Iterator<Item = u8> + '_ {
    overview.iter().step_by(stride(overview)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amplitudes_read_both_layouts() {
        let new = [9, 1, 2, 3, 4, 5, 6, 8, 1, 2, 3, 4, 5, 6];
        assert_eq!(amplitudes(&new).collect::<Vec<_>>(), [9, 8]);
        let mut old = vec![0; COLUMNS * OLD_STRIDE];
        old[OLD_STRIDE] = 7;
        assert_eq!(amplitudes(&old).nth(1), Some(7));
        assert_eq!(amplitudes(&old).count(), COLUMNS);
    }
}
