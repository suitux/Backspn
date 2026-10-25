const MIDDLE: u8 = 1 << 1;
const LOWER_RIGHT: u8 = 1 << 2;
const UPPER_RIGHT: u8 = 1 << 3;
const TOP: u8 = 1 << 4;
const UPPER_LEFT: u8 = 1 << 5;
const LOWER_LEFT: u8 = 1 << 6;
const BOTTOM: u8 = 1 << 7;
pub const DOT: u8 = 1;

pub const BLINK_PERIOD_MS: u128 = 250;
const BLINK_SHOWN_MS: u128 = 125;

fn digit(d: u32) -> u8 {
    match d {
        0 => TOP | UPPER_RIGHT | LOWER_RIGHT | BOTTOM | LOWER_LEFT | UPPER_LEFT,
        1 => UPPER_RIGHT | LOWER_RIGHT,
        2 => TOP | UPPER_RIGHT | MIDDLE | LOWER_LEFT | BOTTOM,
        3 => TOP | UPPER_RIGHT | MIDDLE | LOWER_RIGHT | BOTTOM,
        4 => UPPER_LEFT | MIDDLE | UPPER_RIGHT | LOWER_RIGHT,
        5 => TOP | UPPER_LEFT | MIDDLE | LOWER_RIGHT | BOTTOM,
        6 => TOP | UPPER_LEFT | MIDDLE | LOWER_LEFT | LOWER_RIGHT | BOTTOM,
        7 => TOP | UPPER_RIGHT | LOWER_RIGHT,
        8 => TOP | UPPER_RIGHT | LOWER_RIGHT | BOTTOM | LOWER_LEFT | UPPER_LEFT | MIDDLE,
        9 => TOP | UPPER_RIGHT | LOWER_RIGHT | BOTTOM | UPPER_LEFT | MIDDLE,
        _ => 0,
    }
}

pub fn loop_size_digits(index: u8) -> [u8; 2] {
    if index > 10 {
        return [0, 0];
    }
    let exponent = i32::from(index) - 5;
    let (n, fraction) = if exponent >= 0 {
        (1u32 << exponent, false)
    } else {
        (1u32 << -exponent, true)
    };
    let units = digit(n % 10);
    if n >= 10 {
        let tens = digit(n / 10) | if fraction { DOT } else { 0 };
        [tens, units]
    } else {
        [if fraction { DOT } else { 0 }, units]
    }
}

pub fn loop_display(index: u8, loop_active: bool, live: bool, phase_ms: u128) -> [u8; 2] {
    if live || (loop_active && phase_ms % BLINK_PERIOD_MS > BLINK_SHOWN_MS) {
        return [0, 0];
    }
    loop_size_digits(index)
}

pub fn write_digits(bytes: &mut [u8], first: usize, digits: [u8; 2], on: u8) {
    for (d, mask) in digits.into_iter().enumerate() {
        for bit in 0..8 {
            if let Some(b) = bytes.get_mut(first + 8 * d + bit) {
                *b = if mask & (1 << bit) != 0 { on } else { 0 };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_sizes_show_their_number() {
        assert_eq!(loop_size_digits(5), [0, digit(1)], " 1");
        assert_eq!(loop_size_digits(7), [0, digit(4)], " 4");
        assert_eq!(loop_size_digits(9), [digit(1), digit(6)], "16");
        assert_eq!(loop_size_digits(10), [digit(3), digit(2)], "32");
    }

    #[test]
    fn fractions_show_their_denominator_after_a_dot() {
        assert_eq!(loop_size_digits(4), [DOT, digit(2)], ".2");
        assert_eq!(loop_size_digits(2), [DOT, digit(8)], ".8");
        assert_eq!(loop_size_digits(1), [digit(1) | DOT, digit(6)], "1.6");
        assert_eq!(loop_size_digits(0), [digit(3) | DOT, digit(2)], "3.2");
        assert_eq!(loop_size_digits(11), [0, 0]);
    }

    #[test]
    fn digits_light_the_usual_segments() {
        assert_eq!(digit(1).count_ones(), 2);
        assert_eq!(digit(8).count_ones(), 7);
        assert_eq!(digit(7), TOP | UPPER_RIGHT | LOWER_RIGHT);
        assert_eq!(digit(8) & DOT, 0, "the dot is apart");
    }

    #[test]
    fn an_active_loop_blinks_the_size() {
        assert_eq!(loop_display(7, true, false, 100), loop_size_digits(7));
        assert_eq!(loop_display(7, true, false, 200), [0, 0]);
        assert_eq!(loop_display(7, true, false, 350), loop_size_digits(7));
        assert_eq!(loop_display(7, false, false, 200), loop_size_digits(7));
        assert_eq!(loop_display(7, false, true, 0), [0, 0], "live input");
    }

    #[test]
    fn digits_go_to_eight_bytes_each() {
        let mut bytes = [0u8; 20];
        write_digits(&mut bytes, 2, [DOT, digit(1)], 0x7F);
        assert_eq!(&bytes[2..10], &[0x7F, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&bytes[10..18], &[0, 0, 0x7F, 0x7F, 0, 0, 0, 0]);
    }
}
