use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{PRODUCT_ID, VENDOR_ID};

/// The size of the frames the app draws for a controller's screens.
pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 240;
const FULL_REDRAW_SHARE: (usize, usize) = (3, 4);
const MAX_RECTS: usize = 20;

pub use crate::display::Frame;

/// How a panel takes its pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Rows of 16-bit pixels, blue in the high bits, big-endian.
    Bgr565,
    /// 16-bit pixels, red in the high bits, big-endian, sent as runs of
    /// pixel pairs.
    Rgb565Runs,
}

impl Format {
    fn code(self) -> u8 {
        match self {
            Self::Bgr565 => 0x21,
            Self::Rgb565Runs => 0x60,
        }
    }
}

/// The colour screens of a controller, driven over a USB bulk endpoint.
#[derive(Debug)]
pub struct Panel {
    pub vendor_id: u16,
    pub product_id: u16,
    /// The interface and endpoint of the screens, or `None` to use the
    /// device's first interface with a bulk endpoint out.
    pub interface: Option<(u8, u8)>,
    pub width: usize,
    pub height: usize,
    pub count: usize,
    pub format: Format,
}

pub static S4_PANEL: Panel = Panel {
    vendor_id: VENDOR_ID,
    product_id: PRODUCT_ID,
    interface: Some((4, 0x03)),
    width: WIDTH,
    height: HEIGHT,
    count: 2,
    format: Format::Bgr565,
};

pub fn blank() -> Frame {
    Frame::new(WIDTH, HEIGHT)
}

fn to_wire(rgb: u16) -> u16 {
    let r = rgb >> 11;
    let g = (rgb >> 5) & 0x3f;
    let b = rgb & 0x1f;
    (b << 11) | (g << 5) | r
}

/// A frame the panel takes: its own size, or the app's smaller one to centre.
fn fits(panel: &Panel, frame: &Frame) -> bool {
    let size = (frame.width, frame.height);
    size == (panel.width, panel.height) || size == (WIDTH, HEIGHT)
}

/// The app's frame in the middle of a panel larger than it, black around it.
pub fn fit(panel: &Panel, frame: &Frame) -> Frame {
    if (frame.width, frame.height) == (panel.width, panel.height) {
        return frame.clone();
    }
    let mut out = Frame::new(panel.width, panel.height);
    let left = panel.width.saturating_sub(frame.width) / 2;
    let top = panel.height.saturating_sub(frame.height) / 2;
    for y in 0..frame.height.min(panel.height) {
        let w = frame.width.min(panel.width);
        let at = (top + y) * panel.width + left;
        out.pixels[at..at + w].copy_from_slice(&frame.row(y)[..w]);
    }
    out
}

fn changed_rows(frame: &Frame, previous: Option<&Frame>) -> Vec<(usize, usize)> {
    let height = frame.height;
    let Some(previous) = previous.filter(|p| (p.width, p.height) == (frame.width, height)) else {
        return vec![(0, height)];
    };
    let mut bands: Vec<(usize, usize)> = Vec::new();
    for y in 0..height {
        if frame.row(y) == previous.row(y) {
            continue;
        }
        match bands.last_mut() {
            Some((start, h)) if *start + *h == y => *h += 1,
            _ => bands.push((y, 1)),
        }
    }
    let rows: usize = bands.iter().map(|b| b.1).sum();
    if rows > height * FULL_REDRAW_SHARE.0 / FULL_REDRAW_SHARE.1 || bands.len() > MAX_RECTS {
        return vec![(0, height)];
    }
    bands
}

/// The bulk transfer that draws the rows of `frame` that changed since
/// `previous` on one of the S4 MK3's screens.
pub fn encode(screen: u8, frame: &Frame, previous: Option<&Frame>) -> Vec<u8> {
    encode_for(&S4_PANEL, screen, frame, previous)
}

/// The same for any panel; `frame` has the panel's size.
pub fn encode_for(panel: &Panel, screen: u8, frame: &Frame, previous: Option<&Frame>) -> Vec<u8> {
    let width = frame.width;
    let mut out = Vec::new();
    for (y, h) in changed_rows(frame, previous) {
        let mut header = [0u8; 16];
        header[0] = 0x84;
        header[2] = screen;
        header[3] = panel.format.code();
        header[10..12].copy_from_slice(&(y as u16).to_be_bytes());
        header[12..14].copy_from_slice(&(width as u16).to_be_bytes());
        header[14..16].copy_from_slice(&(h as u16).to_be_bytes());
        out.extend_from_slice(&header);
        let pixels = &frame.pixels[y * width..(y + h) * width];
        match panel.format {
            Format::Bgr565 => {
                for &px in pixels {
                    out.extend_from_slice(&to_wire(px).to_be_bytes());
                }
            }
            Format::Rgb565Runs => {
                let pairs = (pixels.len() / 2) as u32;
                out.extend_from_slice(&(pairs & 0x00FF_FFFF).to_be_bytes());
                for &px in &pixels[..pairs as usize * 2] {
                    out.extend_from_slice(&px.to_be_bytes());
                }
                out.extend_from_slice(&[0x03, 0, 0, 0]);
            }
        }
        out.extend_from_slice(&[0x40, 0, screen, 0]);
    }
    out
}

pub struct Screens {
    panel: &'static Panel,
    frames: Sender<Vec<Frame>>,
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
        Self::start_on(&S4_PANEL)
    }

    pub fn start_on(panel: &'static Panel) -> Result<Self, String> {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<Frame>>();
        let running = Arc::new(AtomicBool::new(true));
        let connected = Arc::new(AtomicBool::new(false));
        let problem = Arc::new(Mutex::new(None));
        let (alive, online, why) = (
            Arc::clone(&running),
            Arc::clone(&connected),
            Arc::clone(&problem),
        );
        let thread = std::thread::Builder::new()
            .name("controller-screens".into())
            .spawn(move || supervise(panel, &rx, &alive, &online, &why))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            panel,
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
        drop(self.frames.send(Vec::from(frames)));
    }
}

impl crate::controller::ScreenSink for Screens {
    fn size(&self) -> (usize, usize) {
        (self.panel.width, self.panel.height)
    }

    fn count(&self) -> usize {
        self.panel.count
    }

    fn connected(&self) -> bool {
        Screens::connected(self)
    }

    fn problem(&self) -> Option<String> {
        Screens::problem(self)
    }

    fn send(&self, frames: Vec<Frame>) {
        if frames.len() >= self.panel.count && frames.iter().all(|f| fits(self.panel, f)) {
            let fitted = frames
                .iter()
                .take(self.panel.count)
                .map(|f| fit(self.panel, f))
                .collect();
            drop(self.frames.send(fitted));
        }
    }
}

const RETRY: Duration = Duration::from_secs(2);
const TIMEOUT: Duration = Duration::from_millis(500);

fn supervise(
    panel: &'static Panel,
    frames: &Receiver<Vec<Frame>>,
    running: &AtomicBool,
    connected: &AtomicBool,
    problem: &Mutex<Option<String>>,
) {
    while running.load(Ordering::Relaxed) {
        let opened = open(panel);
        let why = opened.as_ref().err().filter(|e| *e != ABSENT).cloned();
        if let Ok(mut p) = problem.lock() {
            *p = why;
        }
        if let Ok((handle, interface, endpoint)) = opened {
            connected.store(true, Ordering::Relaxed);
            run(panel, &handle, endpoint, frames, running);
            connected.store(false, Ordering::Relaxed);
            let _ = handle.release_interface(interface);
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

/// The first interface of the device with a bulk endpoint out.
fn bulk_out(device: &rusb::Device<rusb::GlobalContext>) -> Option<(u8, u8)> {
    let config = device.active_config_descriptor().ok()?;
    config.interfaces().find_map(|interface| {
        interface.descriptors().find_map(|d| {
            d.endpoint_descriptors()
                .find(|e| {
                    e.transfer_type() == rusb::TransferType::Bulk
                        && e.direction() == rusb::Direction::Out
                })
                .map(|e| (d.interface_number(), e.address()))
        })
    })
}

type Opened = (rusb::DeviceHandle<rusb::GlobalContext>, u8, u8);

fn open(panel: &Panel) -> Result<Opened, String> {
    let handle = rusb::open_device_with_vid_pid(panel.vendor_id, panel.product_id)
        .ok_or_else(|| ABSENT.to_owned())?;
    let (interface, endpoint) = panel
        .interface
        .or_else(|| bulk_out(&handle.device()))
        .ok_or_else(|| "the controller has no screen interface".to_owned())?;
    let _ = handle.set_auto_detach_kernel_driver(true);
    handle.claim_interface(interface).map_err(|e| match e {
        rusb::Error::Access | rusb::Error::Busy => IN_USE.to_owned(),
        #[cfg(windows)]
        rusb::Error::NotSupported | rusb::Error::NotFound => {
            format!(
                "the screens need the WinUSB driver on the controller's interface {interface} \
                 (README, Controllers)"
            )
        }
        e => e.to_string(),
    })?;
    Ok((handle, interface, endpoint))
}

pub fn probe() -> Result<(), String> {
    let (handle, interface, _) = open(&S4_PANEL)?;
    let _ = handle.release_interface(interface);
    Ok(())
}

fn run(
    panel: &Panel,
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
    endpoint: u8,
    frames: &Receiver<Vec<Frame>>,
    running: &AtomicBool,
) {
    let mut last: Vec<Option<Frame>> = vec![None; panel.count];
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
        for ((screen, frame), previous) in (0u8..).zip(latest).zip(last.iter_mut()) {
            let bytes = encode_for(panel, screen, &frame, previous.as_ref());
            if bytes.is_empty() {
                continue;
            }
            if handle.write_bulk(endpoint, &bytes, TIMEOUT).is_err() {
                return;
            }
            *previous = Some(frame);
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

    static WIDE: Panel = Panel {
        vendor_id: 0,
        product_id: 0,
        interface: None,
        width: 480,
        height: 272,
        count: 2,
        format: Format::Rgb565Runs,
    };

    #[test]
    fn wide_panels_take_runs_of_rgb565_pairs() {
        let mut frame = Frame::new(480, 272);
        frame.pixels[0] = Frame::rgb565(255, 0, 0);
        let bytes = encode_for(&WIDE, 1, &frame, None);
        assert_eq!(
            &bytes[..16],
            &[0x84, 0, 1, 0x60, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0xE0, 1, 0x10]
        );
        let pairs = 480 * 272 / 2;
        assert_eq!(&bytes[16..20], &(pairs as u32).to_be_bytes(), "one run");
        assert_eq!(&bytes[20..22], &[0xF8, 0x00], "red in the high bits");
        let end = bytes.len();
        assert_eq!(end, 16 + 4 + pairs * 4 + 4 + 4);
        assert_eq!(&bytes[end - 8..], &[0x03, 0, 0, 0, 0x40, 0, 1, 0]);
    }

    #[test]
    fn the_app_frame_sits_in_the_middle_of_a_wider_panel() {
        let mut frame = blank();
        frame.pixels[0] = 7;
        let fitted = fit(&WIDE, &frame);
        assert_eq!((fitted.width, fitted.height), (480, 272));
        assert_eq!(fitted.pixels[16 * 480 + 80], 7, "top left of the frame");
        assert_eq!(fitted.pixels[0], 0, "black around it");
        assert_eq!(fit(&S4_PANEL, &frame), frame, "same size: unchanged");
    }

    #[test]
    fn a_panel_takes_frames_of_its_size_or_the_app_size() {
        assert!(fits(&WIDE, &Frame::new(480, 272)));
        assert!(fits(&WIDE, &blank()), "centred");
        assert!(!fits(&WIDE, &Frame::new(100, 100)));
        assert!(fits(&S4_PANEL, &blank()));
        assert!(!fits(&S4_PANEL, &Frame::new(480, 272)), "too big for it");
    }
}
