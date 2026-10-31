use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use super::{Blocks, led_messages};
use crate::InputEvent;
use crate::s4mk3::{Decoder, Layout, LinkStatus, Outputs, RETRY};

type Handle = rusb::DeviceHandle<rusb::GlobalContext>;

const COMMANDS: u8 = 0x01;
const LEDS: u8 = 0x08;
const INPUT: u8 = 0x84;
/// Asks the device who it is; it streams its controls afterwards.
const DEVICE_INFO: u8 = 0x01;
const TIMEOUT: Duration = Duration::from_millis(200);
const READ: Duration = Duration::from_millis(10);
/// Set to print every decoded control to stderr, for testing the hardware.
const TRACE: &str = "BACKSPN_CONTROL_TRACE";

#[cfg(target_os = "macos")]
const IN_USE: &str = "in use by another driver (quit the manufacturer's hardware \
     service; README, Controllers)";
#[cfg(windows)]
const IN_USE: &str = "in use by another driver; it needs the WinUSB driver on its \
     control interface (README, Controllers)";
#[cfg(not(any(target_os = "macos", windows)))]
const IN_USE: &str = "no permission: install the udev rules \
     (packaging/linux/99-backspn.rules) and plug the controller in again";

/// Keeps the controller connected over USB bulk transfers.
pub(crate) fn supervise(
    layout: &'static Layout,
    events: &Sender<(String, InputEvent)>,
    outputs: &Outputs,
    running: &AtomicBool,
    status: &LinkStatus,
    wake: &dyn Fn(),
) {
    let mut leds = None;
    while running.load(Ordering::Relaxed) {
        match open(layout) {
            Ok((handle, interface)) => {
                status.set_problem(None);
                status.connected.store(true, Ordering::Relaxed);
                wake();
                run(layout, &handle, events, outputs, &mut leds, running, wake);
                let _ = handle.release_interface(interface);
                status.connected.store(false, Ordering::Relaxed);
                wake();
            }
            Err(problem) => status.set_problem(problem),
        }
        if let Some(latest) = outputs.leds.try_iter().last() {
            leds = Some(latest.0);
        }
        outputs.drain_unused();
        let until = Instant::now() + RETRY;
        while running.load(Ordering::Relaxed) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

/// The interface that carries the control endpoints.
fn interface_of(device: &rusb::Device<rusb::GlobalContext>) -> Option<u8> {
    let config = device.active_config_descriptor().ok()?;
    config.interfaces().find_map(|interface| {
        interface.descriptors().find_map(|d| {
            d.endpoint_descriptors()
                .any(|e| e.address() == INPUT)
                .then(|| d.interface_number())
        })
    })
}

/// Opens the device. Fails without a reason while it is absent, and with
/// one when it is present but can't be used.
fn open(layout: &Layout) -> Result<(Handle, u8), Option<&'static str>> {
    let device = rusb::devices()
        .ok()
        .and_then(|list| {
            list.iter().find(|d| {
                d.device_descriptor().is_ok_and(|desc| {
                    desc.vendor_id() == layout.vendor_id && desc.product_id() == layout.product_id
                })
            })
        })
        .ok_or(None)?;
    let interface = interface_of(&device).ok_or(Some("its control interface is missing"))?;
    let handle = device.open().map_err(|e| Some(claim_problem(e)))?;
    let _ = handle.set_auto_detach_kernel_driver(true);
    handle
        .claim_interface(interface)
        .map_err(|e| Some(claim_problem(e)))?;
    Ok((handle, interface))
}

fn claim_problem(error: rusb::Error) -> &'static str {
    match error {
        rusb::Error::Access | rusb::Error::Busy => IN_USE,
        #[cfg(windows)]
        rusb::Error::NotSupported | rusb::Error::NotFound => IN_USE,
        _ => "it can't be opened",
    }
}

fn run(
    layout: &'static Layout,
    handle: &Handle,
    events: &Sender<(String, InputEvent)>,
    outputs: &Outputs,
    leds: &mut Option<Vec<u8>>,
    running: &AtomicBool,
    wake: &dyn Fn(),
) {
    let source = layout.source;
    let mut decoder = Decoder::new(layout);
    let mut blocks = Blocks::default();
    let trace = std::env::var_os(TRACE).is_some();
    let _ = handle.write_bulk(COMMANDS, &[DEVICE_INFO], TIMEOUT);
    let mut sent: Option<Vec<u8>> = None;
    let mut buf = [0u8; 512];
    while running.load(Ordering::Relaxed) {
        outputs.drain_unused();
        if let Some(latest) = outputs.leds.try_iter().last() {
            *leds = Some(latest.0);
        }
        if let Some(now) = leds.as_ref().filter(|now| sent.as_ref() != Some(*now)) {
            for message in led_messages(sent.as_deref(), now) {
                if handle.write_bulk(LEDS, &message, TIMEOUT).is_err() {
                    return;
                }
            }
            sent = Some(now.clone());
        }
        let n = match handle.read_bulk(INPUT, &mut buf, READ) {
            Ok(n) => n,
            Err(rusb::Error::Timeout) => 0,
            Err(_) => return,
        };
        let mut woke = false;
        for report in blocks.feed(&buf[..n]) {
            for e in decoder.decode(&report) {
                if trace {
                    eprintln!("{source}: {} = {} (raw {})", e.input, e.value, e.raw);
                }
                drop(events.send((source.into(), e)));
                woke = true;
            }
        }
        if woke {
            wake();
        }
    }
}
