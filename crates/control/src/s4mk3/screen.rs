use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{PRODUCT_ID, VENDOR_ID};

pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 240;
const INTERFACE: u8 = 4;
const ENDPOINT: u8 = 0x03;
const FULL_REDRAW_ROWS: usize = HEIGHT * 3 / 4;
const MAX_RECTS: usize = 20;

pub use crate::display::Frame;

pub fn blank() -> Frame {
    Frame::new(WIDTH, HEIGHT)
}

fn to_wire(rgb: u16) -> u16 {
    let r = rgb >> 11;
    let g = (rgb >> 5) & 0x3f;
    let b = rgb & 0x1f;
    (b << 11) | (g << 5) | r
}

fn changed_rows(frame: &Frame, previous: Option<&Frame>) -> Vec<(usize, usize)> {
    let Some(previous) = previous else {
        return vec![(0, HEIGHT)];
    };
    let mut bands: Vec<(usize, usize)> = Vec::new();
    for y in 0..HEIGHT {
        if frame.row(y) == previous.row(y) {
            continue;
        }
        match bands.last_mut() {
            Some((start, h)) if *start + *h == y => *h += 1,
            _ => bands.push((y, 1)),
        }
    }
    let rows: usize = bands.iter().map(|b| b.1).sum();
    if rows > FULL_REDRAW_ROWS || bands.len() > MAX_RECTS {
        return vec![(0, HEIGHT)];
    }
    bands
}

pub fn encode(screen: u8, frame: &Frame, previous: Option<&Frame>) -> Vec<u8> {
    let mut out = Vec::new();
    for (y, h) in changed_rows(frame, previous) {
        let mut header = [0u8; 16];
        header[0] = 0x84;
        header[2] = screen;
        header[3] = 0x21;
        header[10..12].copy_from_slice(&(y as u16).to_be_bytes());
        header[12..14].copy_from_slice(&(WIDTH as u16).to_be_bytes());
        header[14..16].copy_from_slice(&(h as u16).to_be_bytes());
        out.extend_from_slice(&header);
        for &px in &frame.pixels[y * WIDTH..(y + h) * WIDTH] {
            out.extend_from_slice(&to_wire(px).to_be_bytes());
        }
        out.extend_from_slice(&[0x40, 0, screen, 0]);
    }
    out
}

pub struct Screens {
    frames: Sender<[Frame; 2]>,
    running: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    problem: Arc<Mutex<Option<String>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for Screens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Screens")
            .field("connected", &self.connected())
            .finish_non_exhaustive()
    }
}

impl Drop for Screens {
    /// Waits for the frames already sent to reach the controller, so the last
    /// one stays on the screens after the app quits.
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            drop(thread.join());
        }
    }
}

impl Screens {
    pub fn start() -> Result<Self, String> {
        let (tx, rx) = std::sync::mpsc::channel::<[Frame; 2]>();
        let running = Arc::new(AtomicBool::new(true));
        let connected = Arc::new(AtomicBool::new(false));
        let problem = Arc::new(Mutex::new(None));
        let (alive, online, why) = (
            Arc::clone(&running),
            Arc::clone(&connected),
            Arc::clone(&problem),
        );
        let thread = std::thread::Builder::new()
            .name("s4mk3-screens".into())
            .spawn(move || supervise(&rx, &alive, &online, &why))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            frames: tx,
            running,
            connected,
            problem,
            thread: Some(thread),
        })
    }

    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn problem(&self) -> Option<String> {
        self.problem.lock().ok().and_then(|p| p.clone())
    }

    pub fn send(&self, frames: [Frame; 2]) {
        drop(self.frames.send(frames));
    }
}

impl crate::controller::ScreenSink for Screens {
    fn size(&self) -> (usize, usize) {
        (WIDTH, HEIGHT)
    }

    fn count(&self) -> usize {
        2
    }

    fn connected(&self) -> bool {
        Screens::connected(self)
    }

    fn problem(&self) -> Option<String> {
        Screens::problem(self)
    }

    fn send(&self, frames: Vec<Frame>) {
        if let Ok(pair) = <[Frame; 2]>::try_from(frames)
            && pair.iter().all(|f| (f.width, f.height) == (WIDTH, HEIGHT))
        {
            Screens::send(self, pair);
        }
    }
}

const RETRY: Duration = Duration::from_secs(2);
const TIMEOUT: Duration = Duration::from_millis(500);

fn supervise(
    frames: &Receiver<[Frame; 2]>,
    running: &AtomicBool,
    connected: &AtomicBool,
    problem: &Mutex<Option<String>>,
) {
    while running.load(Ordering::Relaxed) {
        let opened = open();
        let why = opened.as_ref().err().filter(|e| *e != ABSENT).cloned();
        if let Ok(mut p) = problem.lock() {
            *p = why;
        }
        if let Ok(handle) = opened {
            connected.store(true, Ordering::Relaxed);
            run(&handle, frames, running);
            connected.store(false, Ordering::Relaxed);
            let _ = handle.release_interface(INTERFACE);
        }
        let until = std::time::Instant::now() + RETRY;
        while running.load(Ordering::Relaxed) && std::time::Instant::now() < until {
            while frames.try_recv().is_ok() {}
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

const ABSENT: &str = "controller not found";

#[cfg(target_os = "macos")]
const IN_USE: &str = "the screens are in use by another program (on macOS, the \
     controller's own hardware service keeps them while it runs)";
#[cfg(windows)]
const IN_USE: &str = "the screens are in use by another program, or the controller's \
     own driver holds them (README, Controllers)";
#[cfg(not(any(target_os = "macos", windows)))]
const IN_USE: &str = "no permission for the screens: install the udev rules \
     (packaging/linux/99-backspn.rules) and plug the controller in again";

fn open() -> Result<rusb::DeviceHandle<rusb::GlobalContext>, String> {
    let handle =
        rusb::open_device_with_vid_pid(VENDOR_ID, PRODUCT_ID).ok_or_else(|| ABSENT.to_owned())?;
    let _ = handle.set_auto_detach_kernel_driver(true);
    handle.claim_interface(INTERFACE).map_err(|e| match e {
        rusb::Error::Access | rusb::Error::Busy => IN_USE.to_owned(),
        #[cfg(windows)]
        rusb::Error::NotSupported | rusb::Error::NotFound => {
            "the screens need the WinUSB driver on the controller's interface 4 \
             (README, Controllers)"
                .to_owned()
        }
        e => e.to_string(),
    })?;
    Ok(handle)
}

pub fn probe() -> Result<(), String> {
    let handle = open()?;
    let _ = handle.release_interface(INTERFACE);
    Ok(())
}

fn run(
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
    frames: &Receiver<[Frame; 2]>,
    running: &AtomicBool,
) {
    let mut last: [Option<Frame>; 2] = [None, None];
    loop {
        let stopping = !running.load(Ordering::Relaxed);
        let first = if stopping {
            frames.try_recv().ok()
        } else {
            frames.recv_timeout(Duration::from_millis(200)).ok()
        };
        let Some(latest) = first.map(|f| frames.try_iter().last().unwrap_or(f)) else {
            if stopping {
                return;
            }
            continue;
        };
        for (screen, frame) in (0u8..).zip(latest) {
            let previous = last[usize::from(screen)].as_ref();
            let bytes = encode(screen, &frame, previous);
            if bytes.is_empty() {
                continue;
            }
            if handle.write_bulk(ENDPOINT, &bytes, TIMEOUT).is_err() {
                return;
            }
            last[usize::from(screen)] = Some(frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_go_out_as_bgr565_big_endian() {
        let red = Frame::rgb565(255, 0, 0);
        assert_eq!(red, 0xF800);
        assert_eq!(to_wire(red), 0x001F, "red in the low bits");
        assert_eq!(to_wire(Frame::rgb565(0, 0, 255)), 0xF800);
        assert_eq!(to_wire(Frame::rgb565(0, 255, 0)), 0x07E0);
    }

    #[test]
    fn first_frame_is_one_full_rectangle() {
        let mut frame = blank();
        frame.pixels[0] = Frame::rgb565(255, 0, 0);
        let bytes = encode(1, &frame, None);
        assert_eq!(bytes.len(), 16 + WIDTH * HEIGHT * 2 + 4);
        assert_eq!(
            &bytes[..16],
            &[0x84, 0, 1, 0x21, 0, 0, 0, 0, 0, 0, 0, 0, 1, 64, 0, 240]
        );
        assert_eq!(&bytes[16..18], &[0x00, 0x1F], "first pixel, big-endian");
        assert_eq!(&bytes[bytes.len() - 4..], &[0x40, 0, 1, 0]);
    }

    #[test]
    fn only_changed_rows_are_sent() {
        let before = blank();
        assert_eq!(encode(0, &before, Some(&before)), Vec::<u8>::new());
        let mut after = before.clone();
        after.pixels[10 * WIDTH + 5] = 1;
        after.pixels[11 * WIDTH] = 1;
        after.pixels[100 * WIDTH] = 1;
        let bytes = encode(0, &after, Some(&before));
        let rect = |rows: usize| 16 + rows * WIDTH * 2 + 4;
        assert_eq!(bytes.len(), rect(2) + rect(1));
        assert_eq!(&bytes[10..16], &[0, 10, 1, 64, 0, 2], "rows 10–11");
        let second = rect(2);
        assert_eq!(&bytes[second + 10..second + 16], &[0, 100, 1, 64, 0, 1]);
    }

    #[test]
    fn many_changes_become_a_full_redraw() {
        let before = blank();
        let mut after = before.clone();
        for y in (0..HEIGHT).step_by(2) {
            after.pixels[y * WIDTH] = 1;
        }
        let bytes = encode(0, &after, Some(&before));
        assert_eq!(bytes.len(), 16 + WIDTH * HEIGHT * 2 + 4);
    }
}
