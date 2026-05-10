mod hid;
mod tuning;

use hid::REPORT_SIZE;
use tuning::{build_request_report, is_tuning_report, parse_tuning_report};

fn main() {
    let devices = match hid::enumerate_fanatec() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: failed to enumerate HID devices: {}", e);
            std::process::exit(1);
        }
    };

    if devices.is_empty() {
        eprintln!("No Fanatec devices found (VID 0x0EB7).");
        eprintln!("Check that the device is connected and powered on.");
        eprintln!("If FanaLab is running with exclusive access, close it and retry.");
        std::process::exit(1);
    }

    println!("Fanatec devices found:");
    for (i, dev) in devices.iter().enumerate() {
        println!(
            "  [{}] {} (PID 0x{:04X})  {}",
            i, dev.product_name, dev.product_id, dev.device_path
        );
    }
    println!();

    // Try each collection in turn; the tuning endpoint rejects writes with
    // ERROR_INVALID_FUNCTION (1) on the wrong collection.
    match probe_tuning_collection(&devices) {
        Some((idx, buf)) => {
            let col = collection_label(&devices[idx].device_path);
            println!(
                "Using {} for tuning commands  ({}, PID 0x{:04X})",
                col, devices[idx].product_name, devices[idx].product_id
            );
            print_profile(&parse_tuning_report(&buf));
        }
        None => {
            eprintln!("error: no HID collection responded to the tuning request.");
            eprintln!(
                "Tried all {} collection(s). Close FanaLab and retry.",
                devices.len()
            );
            std::process::exit(1);
        }
    }
}

/// Tries each device handle in order.  Sends the tuning-values request,
/// then waits for a tuning report.  Returns the device index and the raw
/// 64-byte report on success.
fn probe_tuning_collection(devices: &[hid::FanatecDevice]) -> Option<(usize, [u8; REPORT_SIZE])> {
    let request = build_request_report();

    for (idx, dev) in devices.iter().enumerate() {
        let col = collection_label(&dev.device_path);
        print!("  Probing {} ... ", col);

        if let Err(e) = hid::write_report(dev, &request) {
            println!("write failed ({})", e);
            continue;
        }

        // Write succeeded; now wait for the device to send back a tuning report.
        // Other IN reports (e.g. wheel position) may arrive first, so retry.
        let mut buf = [0u8; REPORT_SIZE];
        let mut got_tuning = false;
        for _ in 0..10 {
            match hid::read_report(dev, &mut buf, 200) {
                Ok(()) if is_tuning_report(&buf) => {
                    got_tuning = true;
                    break;
                }
                Ok(()) => {} // different report type, keep polling
                Err(e) => {
                    println!("read failed ({})", e);
                    break;
                }
            }
        }

        if got_tuning {
            println!("OK");
            return Some((idx, buf));
        } else {
            println!("no tuning response");
        }
    }

    None
}

/// Extracts the "col01" / "col02" label from a HID device path.
/// Paths look like: \\?\hid#vid_0eb7&pid_0020&col02#6&...
fn collection_label(path: &str) -> String {
    let lower = path.to_lowercase();
    if let Some(amp) = lower.find("&col") {
        let start = amp + 1; // skip '&', point at 'c'
        let end = lower[start..]
            .find('#')
            .map(|i| start + i)
            .unwrap_or(start + 5);
        return path[start..end.min(start + 5)].to_string();
    }
    "collection".to_string()
}

fn print_profile(p: &tuning::TuningProfile) {
    println!();
    println!("Slot:  {}", p.slot);

    match p.sen_degrees {
        Some(0) | None => println!("SEN:   AUTO      (raw 0x{:02X})", p.sen_raw),
        Some(deg) => println!("SEN:   {}°      (raw 0x{:02X})", deg, p.sen_raw),
    }

    println!("FF:    {}", p.ff);

    let ffs_label = match p.ffs {
        0 => "LINEAR",
        1 => "PEAK",
        _ => "?",
    };
    println!("FFS:   {} ({})", ffs_label, p.ffs);

    println!("NDP:   {}", p.ndp);
    println!("NFR:   {}", p.nfr);
    println!("NIN:   {}", p.nin);
    println!("INT:   {}", p.int_);
    println!("FEI:   {}", p.fei);

    println!("FOR:   {} (wire {})", p.for_, p.for_ / 10);
    println!("SPR:   {} (wire {})", p.spr, p.spr / 10);
    println!("DPR:   {} (wire {})", p.dpr, p.dpr / 10);

    let bli_label = if p.bli == 101 {
        "OFF".to_string()
    } else {
        p.bli.to_string()
    };
    println!("BLI:   {}", bli_label);

    println!("SHO:   {} (wire {})", p.sho, p.sho / 10);
    println!("BRF:   {}", p.brf);

    println!("ACP:   {}", p.acp);
    println!("FUL:   {}", p.ful);
    println!("DRI:   {}", p.dri);
}
