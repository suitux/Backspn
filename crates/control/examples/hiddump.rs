use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let seconds: u64 = args.next().map_or(Ok(10), |s| s.parse())?;
    let vid = args
        .next()
        .map_or(Ok(0x17cc), |s| u16::from_str_radix(&s, 16))?;
    let pid = args
        .next()
        .map_or(Ok(0x1720), |s| u16::from_str_radix(&s, 16))?;
    let api = hidapi::HidApi::new()?;
    for d in api
        .device_list()
        .filter(|d| d.vendor_id() == vid && d.product_id() == pid)
    {
        println!(
            "found {:?} usage page {:#06x} usage {:#06x} interface {} path {:?}",
            d.product_string(),
            d.usage_page(),
            d.usage(),
            d.interface_number(),
            d.path()
        );
    }
    let device = api.open(vid, pid)?;
    device.set_blocking_mode(false)?;
    let mut last: std::collections::HashMap<u8, Vec<u8>> = std::collections::HashMap::new();
    let mut buf = [0u8; 256];
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(seconds) {
        let n = device.read_timeout(&mut buf, 50)?;
        if n == 0 {
            continue;
        }
        let report = &buf[..n];
        let id = report[0];
        let previous = last.insert(id, report.to_vec());
        match previous {
            None => println!(
                "{:8.3} report {id:#04x} ({n} bytes) first: {}",
                started.elapsed().as_secs_f64(),
                hex(report)
            ),
            Some(prev) => {
                let changes: Vec<String> = report
                    .iter()
                    .zip(&prev)
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .map(|(i, (a, b))| format!("[{i}] {b:02x}→{a:02x}"))
                    .collect();
                if !changes.is_empty() {
                    println!(
                        "{:8.3} report {id:#04x}: {}",
                        started.elapsed().as_secs_f64(),
                        changes.join(" ")
                    );
                }
            }
        }
    }
    Ok(())
}

fn hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
