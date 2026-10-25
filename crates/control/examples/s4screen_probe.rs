use backspn_control::s4mk3::screen::{self, Frame, HEIGHT, WIDTH};

fn main() {
    match screen::probe() {
        Ok(()) => println!("screen interface is free"),
        Err(e) => {
            println!("cannot open the screens: {e}");
            return;
        }
    }
    let mut frames = [screen::blank(), screen::blank()];
    for (i, frame) in frames.iter_mut().enumerate() {
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let (r, g, b) = if i == 0 {
                    ((x * 255 / WIDTH) as u8, (y * 255 / HEIGHT) as u8, 64)
                } else {
                    (64, (y * 255 / HEIGHT) as u8, (x * 255 / WIDTH) as u8)
                };
                frame.pixels[y * WIDTH + x] = Frame::rgb565(r, g, b);
            }
        }
    }
    match screen::Screens::start() {
        Ok(screens) => {
            for _ in 0..30 {
                screens.send(frames.clone());
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            println!("sent a test pattern (connected: {})", screens.connected());
        }
        Err(e) => println!("cannot start: {e}"),
    }
}
