#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub pixels: Box<[u16]>,
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Frame({}×{})", self.width, self.height)
    }
}

impl Frame {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height].into_boxed_slice(),
        }
    }

    pub fn rgb565(r: u8, g: u8, b: u8) -> u16 {
        (u16::from(r >> 3) << 11) | (u16::from(g >> 2) << 5) | u16::from(b >> 3)
    }

    pub fn row(&self, y: usize) -> &[u16] {
        &self.pixels[y * self.width..(y + 1) * self.width]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_have_their_size_and_rows() {
        let mut f = Frame::new(4, 3);
        f.pixels[5] = Frame::rgb565(255, 0, 0);
        assert_eq!(f.row(1), [0, 0xF800, 0, 0]);
        assert_eq!(Frame::rgb565(0, 0, 255), 0x001F);
    }
}
