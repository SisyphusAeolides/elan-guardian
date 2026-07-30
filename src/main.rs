use elan_guardian::device::{affected_thinkpad_p53, discover};
use elan_guardian::diagnose::analyze_trace;
use elan_guardian::irq;
use elan_guardian::recover::{rebind_controller, recover};
use elan_guardian::trace::{self, RecordOptions};
use elan_guardian::watch;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

fn main() -> ExitCode {
    match run(&env::args_os().skip(1).collect::<Vec<_>>()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("elan-guardian: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[OsString]) -> Result<(), String> {
    let Some(command) = args.first().and_then(|arg| arg.to_str()) else {
        print_help();
        return Ok(());
    };
    match command {
        "--help" | "-h" | "help" => print_help(),
        "--version" | "-V" => println!("elan-guardian {}", env!("CARGO_PKG_VERSION")),
        "status" => status(&args[1..])?,
        "record" => record_command(&args[1..])?,
        "analyze" => analyze_command(&args[1..])?,
        "export-features" => export_features(&args[1..])?,
        "recover" => recover_command(&args[1..])?,
        "watch" => watch_command(&args[1..])?,
        other => return Err(format!("unknown command '{other}' (try --help)")),
    }
    Ok(())
}

fn status(args: &[OsString]) -> Result<(), String> {
    let json = args.iter().any(|arg| arg == "--json");
    let controllers = discover(Path::new("/sys")).map_err(|error| error.to_string())?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&controllers).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if controllers.is_empty() {
        return Err("no controllers are bound to elan_i2c".into());
    }
    let irq_total = irq::total_from_proc(Path::new("/proc/interrupts")).ok();
    for controller in controllers {
        println!("Controller: {}", controller.id);
        println!(
            "  Product: {}",
            controller.product_id.as_deref().unwrap_or("unknown")
        );
        println!(
            "  Firmware: {}",
            controller.firmware_version.as_deref().unwrap_or("unknown")
        );
        println!(
            "  IAP: {}",
            controller.iap_version.as_deref().unwrap_or("unknown")
        );
        println!(
            "  Runtime watchdog: {}",
            controller
                .runtime_watchdog
                .as_deref()
                .unwrap_or("unavailable")
        );
        println!(
            "  IRQ total: {}",
            irq_total
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".into())
        );
        println!(
            "  Events: {}",
            controller
                .event_nodes
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}

fn record_command(args: &[OsString]) -> Result<(), String> {
    let mut options = RecordOptions::default();
    let mut output = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].to_str() {
            Some("--duration") => {
                index += 1;
                options.duration = Duration::from_secs(parse_u64(args.get(index), "--duration")?);
            }
            Some("--interval-ms") => {
                index += 1;
                options.interval =
                    Duration::from_millis(parse_u64(args.get(index), "--interval-ms")?);
            }
            Some("--output") | Some("-o") => {
                index += 1;
                output = args.get(index).map(PathBuf::from);
            }
            Some("--expect-motion") => options.expect_motion = true,
            Some("--cursor-stalled") => options.cursor_stalled = true,
            Some(option) => return Err(format!("unknown record option '{option}'")),
            None => return Err("record arguments must be valid UTF-8".into()),
        }
        index += 1;
    }
    let output = output.ok_or_else(|| "record requires --output PATH".to_string())?;
    let captured = trace::record(&options).map_err(|error| error.to_string())?;
    trace::write_json(&output, &captured).map_err(|error| error.to_string())?;
    let diagnosis = analyze_trace(&captured);
    println!("Trace: {}", output.display());
    println!("Diagnosis: {}", diagnosis.kind);
    println!("{}", diagnosis.explanation);
    Ok(())
}

fn analyze_command(args: &[OsString]) -> Result<(), String> {
    let path = args
        .iter()
        .find(|arg| *arg != "--json")
        .map(PathBuf::from)
        .ok_or_else(|| "analyze requires TRACE.json".to_string())?;
    let diagnosis = analyze_trace(&trace::read_json(&path).map_err(|error| error.to_string())?);
    if args.iter().any(|arg| arg == "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&diagnosis).map_err(|e| e.to_string())?
        );
    } else {
        println!("Diagnosis: {}", diagnosis.kind);
        println!(
            "IRQ delta: {}",
            diagnosis
                .irq_delta
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".into())
        );
        println!("Evdev byte delta: {}", diagnosis.event_byte_delta);
        println!("{}", diagnosis.explanation);
    }
    Ok(())
}

fn export_features(args: &[OsString]) -> Result<(), String> {
    let [input, output] = args else {
        return Err("export-features requires TRACE.json OUTPUT.dat".into());
    };
    let trace = trace::read_json(Path::new(input)).map_err(|error| error.to_string())?;
    let diagnosis = analyze_trace(&trace);
    let row = format!(
        "{} {} {} {}\n",
        diagnosis.irq_delta.unwrap_or(0),
        diagnosis.event_byte_delta,
        u8::from(trace.expect_motion),
        u8::from(trace.cursor_stalled)
    );
    fs::write(output, row).map_err(|error| error.to_string())?;
    Ok(())
}

fn recover_command(args: &[OsString]) -> Result<(), String> {
    let mut requested = Vec::new();
    let mut affected_only = false;
    let mut quiet = false;
    let mut force_rebind = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].to_str() {
            Some("--device") => {
                index += 1;
                let id = args
                    .get(index)
                    .and_then(|arg| arg.to_str())
                    .ok_or_else(|| "--device requires an I2C identifier".to_string())?;
                requested.push(id.to_string());
            }
            Some("--all") => {}
            Some("--affected-only") => affected_only = true,
            Some("--quiet") => quiet = true,
            Some("--rebind") => force_rebind = true,
            Some(option) => return Err(format!("unknown recover option '{option}'")),
            None => return Err("recover arguments must be valid UTF-8".into()),
        }
        index += 1;
    }
    if affected_only && !affected_thinkpad_p53(Path::new("/sys")) {
        return Ok(());
    }
    if unsafe { libc::geteuid() } != 0 {
        return Err("recover must run as root".into());
    }
    if requested.is_empty() {
        requested = discover(Path::new("/sys"))
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|controller| controller.id)
            .collect();
    }
    if requested.is_empty() {
        return Err("no controllers are bound to elan_i2c".into());
    }
    for id in requested {
        let recovered = if force_rebind {
            rebind_controller(Path::new("/sys"), &id)
        } else {
            recover(Path::new("/sys"), &id)
        }
        .map_err(|error| error.to_string())?;
        if !quiet {
            println!(
                "Recovered {} via {:?} ({})",
                recovered.id,
                recovered.method,
                recovered.event_nodes.join(", ")
            );
        }
    }
    Ok(())
}

fn watch_command(args: &[OsString]) -> Result<(), String> {
    let mut affected_only = false;
    let mut interval = Duration::from_secs(1);
    let mut index = 0;
    while index < args.len() {
        match args[index].to_str() {
            Some("--affected-only") => affected_only = true,
            Some("--interval-ms") => {
                index += 1;
                interval = Duration::from_millis(parse_u64(args.get(index), "--interval-ms")?);
                if interval.is_zero() {
                    return Err("--interval-ms must be greater than zero".into());
                }
            }
            Some(option) => return Err(format!("unknown watch option '{option}'")),
            None => return Err("watch arguments must be valid UTF-8".into()),
        }
        index += 1;
    }
    if unsafe { libc::geteuid() } != 0 {
        return Err("watch must run as root".into());
    }
    if affected_only && !affected_thinkpad_p53(Path::new("/sys")) {
        return Ok(());
    }
    watch::monitor(Path::new("/sys"), interval).map_err(|error| error.to_string())
}

fn parse_u64(value: Option<&OsString>, option: &str) -> Result<u64, String> {
    value
        .and_then(|arg| arg.to_str())
        .and_then(|arg| arg.parse().ok())
        .ok_or_else(|| format!("{option} requires a non-negative integer"))
}

fn print_help() {
    println!(
        "elan-guardian {}\n\
         Evidence-driven Elantech I2C diagnostics and recovery\n\n\
         Usage:\n\
           elan-guardian status [--json]\n\
           elan-guardian record --output TRACE.json [--duration SECONDS]\n\
               [--interval-ms MS] [--expect-motion] [--cursor-stalled]\n\
           elan-guardian analyze TRACE.json [--json]\n\
           elan-guardian export-features TRACE.json OUTPUT.dat\n\
           elan-guardian recover [--all | --device I2C-ID] [--affected-only]
               [--rebind] [--quiet]
           elan-guardian watch [--affected-only] [--interval-ms MS]",
        env!("CARGO_PKG_VERSION")
    );
}
