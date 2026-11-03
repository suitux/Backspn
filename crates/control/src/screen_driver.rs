//! Setting up a controller's screen interface for raw USB on Windows.
//!
//! Windows gives the screen interface to the manufacturer's driver, and raw
//! USB reaches it only through `WinUSB` with an interface class registered.
//! `screen_driver.ps1` does both with the `WinUSB` driver that ships with
//! Windows; this module runs it elevated, one setup at a time.

use std::sync::{Mutex, PoisonError};

use base64::Engine as _;

const SCRIPT: &str = include_str!("screen_driver.ps1");

/// The USB interface of a controller's screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interface {
    pub vendor_id: u16,
    pub product_id: u16,
    pub number: u8,
}

impl Interface {
    /// The interface's hardware ID, the start of its device instance IDs.
    pub fn hardware_id(self) -> String {
        format!(
            "USB\\VID_{:04X}&PID_{:04X}&MI_{:02X}",
            self.vendor_id, self.product_id, self.number
        )
    }
}

/// Why a setup did not finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The administrator prompt was declined.
    Declined,
    /// The controller was unplugged.
    Absent,
    /// Windows did not install the driver.
    Driver,
    /// The interface could not be registered.
    Register,
    /// PowerShell could not run, or ended some other way.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setup {
    Idle,
    Running,
    Done,
    Failed(Failure),
}

static STATE: Mutex<Setup> = Mutex::new(Setup::Idle);

pub fn state() -> Setup {
    *STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn set_state(setup: Setup) {
    *STATE.lock().unwrap_or_else(PoisonError::into_inner) = setup;
}

/// The script for one interface; the hardware ID holds only letters, digits,
/// `\`, `_` and `&`, so it is safe inside single quotes.
fn script_for(interface: Interface) -> String {
    format!("$HardwareId = '{}'\n{SCRIPT}", interface.hardware_id())
}

/// A script as PowerShell's `-EncodedCommand` takes it: base64 of UTF-16LE.
fn encoded(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// The command that asks for administrator rights and runs the script;
/// declining the prompt ends it with code 2.
#[cfg_attr(
    not(windows),
    allow(
        dead_code,
        reason = "only Windows runs the setup; the tests use it everywhere"
    )
)]
fn elevating_command(script: &str) -> String {
    format!(
        "try {{ $p = Start-Process -FilePath powershell.exe -Verb RunAs -Wait -PassThru \
         -WindowStyle Hidden -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand','{}'; \
         exit $p.ExitCode }} catch {{ exit 2 }}",
        encoded(script)
    )
}

fn outcome(code: Option<i32>) -> Setup {
    match code {
        Some(0) => Setup::Done,
        Some(2) => Setup::Failed(Failure::Declined),
        Some(3) => Setup::Failed(Failure::Absent),
        Some(4) => Setup::Failed(Failure::Driver),
        Some(5) => Setup::Failed(Failure::Register),
        _ => Setup::Failed(Failure::Other),
    }
}

/// Sets up `interface` in the background, unless a setup is running; the
/// screens find the interface on their next look once it is done.
pub fn start(interface: Interface) {
    {
        let mut state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
        if *state == Setup::Running {
            return;
        }
        *state = Setup::Running;
    }
    let spawned = std::thread::Builder::new()
        .name("screen-driver".into())
        .spawn(move || set_state(outcome(run(&script_for(interface)))));
    if spawned.is_err() {
        set_state(Setup::Failed(Failure::Other));
    }
}

#[cfg(windows)]
fn run(script: &str) -> Option<i32> {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg(elevating_command(script))
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .ok()
        .and_then(|s| s.code())
}

#[cfg(not(windows))]
fn run(_script: &str) -> Option<i32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const S4: Interface = Interface {
        vendor_id: 0x17CC,
        product_id: 0x1720,
        number: 4,
    };

    #[test]
    fn the_hardware_id_names_vendor_product_and_interface_in_hex() {
        assert_eq!(S4.hardware_id(), r"USB\VID_17CC&PID_1720&MI_04");
        let wide = Interface {
            vendor_id: 0x17CC,
            product_id: 0x1370,
            number: 0x0B,
        };
        assert_eq!(wide.hardware_id(), r"USB\VID_17CC&PID_1370&MI_0B");
    }

    #[test]
    fn the_script_starts_by_naming_the_interface() {
        let script = script_for(S4);
        assert!(script.starts_with("$HardwareId = 'USB\\VID_17CC&PID_1720&MI_04'\n"));
        assert!(script.ends_with(SCRIPT));
    }

    #[test]
    fn scripts_are_encoded_as_base64_of_utf16le() {
        assert_eq!(encoded("exit 0"), "ZQB4AGkAdAAgADAA");
        assert_eq!(encoded(""), "");
    }

    #[test]
    fn the_encoded_script_fits_a_windows_command_line() {
        assert!(elevating_command(&script_for(S4)).len() < 32_000);
    }

    #[test]
    fn exit_codes_tell_how_the_setup_ended() {
        assert_eq!(outcome(Some(0)), Setup::Done);
        assert_eq!(outcome(Some(2)), Setup::Failed(Failure::Declined));
        assert_eq!(outcome(Some(3)), Setup::Failed(Failure::Absent));
        assert_eq!(outcome(Some(4)), Setup::Failed(Failure::Driver));
        assert_eq!(outcome(Some(5)), Setup::Failed(Failure::Register));
        assert_eq!(outcome(Some(1)), Setup::Failed(Failure::Other));
        assert_eq!(outcome(None), Setup::Failed(Failure::Other));
    }
}
