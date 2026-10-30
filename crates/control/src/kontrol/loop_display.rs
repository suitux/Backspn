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

/// The loop length of a size index: the number shown and whether it is a
/// fraction of a beat (1/n).
fn loop_size(index: u8) -> Option<(u32, bool)> {
    if index > 10 {
        return None;
    }
    let exponent = i32::from(index) - 5;
    Some(if exponent >= 0 {
        (1u32 << exponent, false)
    } else {
        (1u32 << -exponent, true)
    })
}

pub fn loop_size_digits(index: u8) -> [u8; 2] {
    let Some((n, fraction)) = loop_size(index) else {
        return [0, 0];
    };
    let units = digit(n % 10);
    if n >= 10 {
        let tens = digit(n / 10) | if fraction { DOT } else { 0 };
        [tens, units]
    } else {
        [if fraction { DOT } else { 0 }, units]
    }
}

/// Tens and units of a number from 0 to 99, the tens blank below 10.
pub fn number_digits(n: u8) -> [u8; 2] {
    let n = u32::from(n.min(99));
    let tens = if n >= 10 { digit(n / 10) } else { 0 };
    [tens, digit(n % 10)]
}

/// Three digits without dots: whole sizes right-aligned, a fraction 1/n as a
/// dash before n.
pub fn loop_size_three_digits(index: u8) -> [u8; 3] {
    let Some((n, fraction)) = loop_size(index) else {
        return [0; 3];
    };
    let tens = if n >= 10 { digit(n / 10) } else { 0 };
    [if fraction { MIDDLE } else { 0 }, tens, digit(n % 10)]
}

/// Whether the loop size is blanked right now: always with live input, and
/// for part of each period while the loop plays.
pub fn hidden(loop_active: bool, live: bool, phase_ms: u128) -> bool {
    live || (loop_active && phase_ms % BLINK_PERIOD_MS > BLINK_SHOWN_MS)
}

pub fn loop_display(index: u8, loop_active: bool, live: bool, phase_ms: u128) -> [u8; 2] {
    if hidden(loop_active, live, phase_ms) {
        return [0, 0];
    }
    loop_size_digits(index)
}

/// Milliseconds into the current blink period, shared by every display.
pub fn phase_ms() -> u128 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis()
        % BLINK_PERIOD_MS
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

/// Digits of seven LEDs each (no dot): segment bit k lights LED k - 1.
pub fn write_seven_segments(bytes: &mut [u8], first: usize, digits: &[u8], on: u8) {
    for (d, mask) in digits.iter().enumerate() {
        for bit in 1..8 {
            if let Some(b) = bytes.get_mut(first + 7 * d + bit - 1) {
                *b = if mask & (1 << bit) != 0 { on } else { 0 };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_blank_the_leading_zero() {
        assert_eq!(number_digits(7), [0, digit(7)]);
        assert_eq!(number_digits(42), [digit(4), digit(2)]);
        assert_eq!(number_digits(250), [digit(9), digit(9)]);
    }

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
    fn three_digits_dash_the_fractions() {
        assert_eq!(loop_size_three_digits(7), [0, 0, digit(4)], "  4");
        assert_eq!(loop_size_three_digits(10), [0, digit(3), digit(2)], " 32");
        assert_eq!(loop_size_three_digits(4), [MIDDLE, 0, digit(2)], "- 2");
        assert_eq!(
            loop_size_three_digits(1),
            [MIDDLE, digit(1), digit(6)],
            "-16"
        );
        assert_eq!(loop_size_three_digits(11), [0; 3]);
    }

    #[test]
    fn seven_segment_digits_skip_the_dot() {
        let mut bytes = [0u8; 16];
        write_seven_segments(&mut bytes, 1, &[digit(1) | DOT, MIDDLE], 0x7F);
        assert_eq!(&bytes[1..8], &[0, 0x7F, 0x7F, 0, 0, 0, 0], "1, dot dropped");
        assert_eq!(&bytes[8..15], &[0x7F, 0, 0, 0, 0, 0, 0], "dash");
    }

    #[test]
    fn digits_go_to_eight_bytes_each() {
        let mut bytes = [0u8; 20];
        write_digits(&mut bytes, 2, [DOT, digit(1)], 0x7F);
        assert_eq!(&bytes[2..10], &[0x7F, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&bytes[10..18], &[0, 0, 0x7F, 0x7F, 0, 0, 0, 0]);
    }
}
