mod hid;
mod tuning;

use hid::{FanatecDevice, REPORT_SIZE};
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

    // Use the first device (lowest index is usually the wheel base, not a pedal unit)
    let dev = &devices[0];
    println!(
        "Reading tuning profile from: {} (PID 0x{:04X})",
        dev.product_name, dev.product_id
    );

    if let Err(e) = read_and_print_tuning(dev) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn read_and_print_tuning(dev: &FanatecDevice) -> Result<(), String> {
    let request = build_request_report();
    hid::write_report(dev, &request).map_err(|e| format!("write failed: {}", e))?;

    // The device may send other input reports before the tuning report.
    // Poll up to 10 times with a 200 ms timeout each.
    let mut buf = [0u8; REPORT_SIZE];
    let mut found = false;
    for attempt in 0..10 {
        match hid::read_report(dev, &mut buf, 200) {
            Ok(()) => {
                if is_tuning_report(&buf) {
                    found = true;
                    break;
                }
                // Not our report; keep polling
            }
            Err(hid::HidError::Timeout) => {
                if attempt == 0 {
                    return Err("device did not respond to tuning request (timeout)".into());
                }
                break;
            }
            Err(e) => return Err(format!("read failed: {}", e)),
        }
    }

    if !found {
        return Err("device did not send a tuning report after 10 attempts".into());
    }

    let p = parse_tuning_report(&buf);
    print_profile(&p);
    Ok(())
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

    let bli_label = if p.bli == 101 { "OFF".to_string() } else { p.bli.to_string() };
    println!("BLI:   {}", bli_label);

    println!("SHO:   {} (wire {})", p.sho, p.sho / 10);
    println!("BRF:   {}", p.brf);

    println!("ACP:   {}", p.acp);
    println!("FUL:   {}", p.ful);
    println!("DRI:   {}", p.dri);
}
