// Copyright 2025 Adam Greig
// Licensed under Apache-2.0 and MIT licenses.

use std::{io::Write, fs::File, time::{Instant, Duration}};
use clap::{Command, Arg, ArgAction, crate_description, crate_version, value_parser};
use clap_num::{maybe_hex, si_number};
use anyhow::bail;

use jtagdap::probe::{Probe, ProbeInfo};
use jtagdap::dap::DAP;
use jtagdap::jtag::{JTAG, JTAGChain};
use x7dap::{check_tap_idx, auto_tap_idx, X7IDCODE, X7};

fn main() -> anyhow::Result<()> {
    let matches = Command::new("x7dap")
        .version(crate_version!())
        .about(crate_description!())
        .subcommand_required(true)
        .arg_required_else_help(true)
        .propagate_version(true)
        .infer_subcommands(true)
        .arg(Arg::new("quiet")
             .help("Suppress informative output and raise log level to errors only")
             .long("quiet")
             .short('q')
             .action(ArgAction::SetTrue)
             .global(true))
        .arg(Arg::new("verbose")
             .help("Increase log level, specify once for info, twice for debug, three times for trace")
             .long("verbose")
             .short('v')
             .action(ArgAction::Count)
             .conflicts_with("quiet")
             .global(true))
        .arg(Arg::new("probe")
             .help("VID:PID[:SN] of CMSIS-DAP device to use")
             .long("probe")
             .short('p')
             .action(ArgAction::Set)
             .global(true))
        .arg(Arg::new("freq")
             .help("JTAG clock frequency in Hz (k and M suffixes allowed)")
             .long("freq")
             .short('f')
             .action(ArgAction::Set)
             .default_value("1M")
             .value_parser(si_number::<u32>)
             .global(true))
        .arg(Arg::new("tap")
             .help("Device's TAP position in scan chain (0-indexed, see `scan` output)")
             .long("tap")
             .short('t')
             .action(ArgAction::Set)
             .value_parser(value_parser!(usize))
             .global(true))
        .arg(Arg::new("ir-lengths")
             .help("Lengths of each IR, starting from TAP 0, comma-separated")
             .long("ir-lengths")
             .short('i')
             .action(ArgAction::Set)
             .value_delimiter(',')
             .value_parser(value_parser!(usize))
             .global(true))
        .arg(Arg::new("scan-chain-length")
             .help("Maximum JTAG scan chain length to check")
             .long("scan-chain-length")
             .short('l')
             .action(ArgAction::Set)
             .default_value("192")
             .value_parser(value_parser!(usize))
             .global(true))
        .subcommand(Command::new("probes")
            .about("List available CMSIS-DAP probes"))
        .subcommand(Command::new("scan")
            .about("Scan JTAG chain and detect 7-series IDCODEs"))
        .subcommand(Command::new("reset")
            .about("Pulse the JTAG nRST line for 100ms"))
        .subcommand(Command::new("reload")
            .about("Request the device reload its configuration"))
        .subcommand(Command::new("dna")
            .about("Read the device DNA"))
        .subcommand(Command::new("status")
            .about("Read the device status"))
        .subcommand(Command::new("program")
            .about("Program SRAM with bitstream")
            .arg(Arg::new("file")
                 .help("File to program to device")
                 .required(true))
            .arg(Arg::new("remove-spimode")
                .help("Disable removing SPI_MODE commands when writing bitstreams to SRAM")
                .long("no-remove-spimode")
                .action(ArgAction::SetFalse)
                .global(true)))
        .get_matches();

    let t0 = Instant::now();
    let quiet = matches.get_flag("quiet");
    let verbose = matches.get_count("verbose");
    let env = if quiet {
        env_logger::Env::default().default_filter_or("error")
    } else if verbose == 0 {
        env_logger::Env::default().default_filter_or("warn")
    } else if verbose == 1 {
        env_logger::Env::default().default_filter_or("info")
    } else if verbose == 2 {
        env_logger::Env::default().default_filter_or("debug")
    } else {
        env_logger::Env::default().default_filter_or("trace")
    };
    env_logger::Builder::from_env(env).format_timestamp(None).init();

    // Listing probes does not require first connecting to a probe,
    // so we just list them and quit early.
    if matches.subcommand_name().unwrap() == "probes" {
        print_probe_list();
        return Ok(());
    }

    // All functions after this point require an open probe, so
    // we now attempt to connect to the specified probe.
    let probe = if let Some(probe) = matches.get_one::<String>("probe") {
        ProbeInfo::from_specifier(probe)?.open()?
    } else {
        Probe::new()?
    };

    // Create a JTAG interface using the probe.
    let dap = DAP::new(probe)?;
    let mut jtag = JTAG::new(dap);

    // At this point we can handle the reset command.
    if matches.subcommand_name().unwrap() == "reset" {
        if !quiet { println!("Pulsing nRST line.") };
        return Ok(jtag.pulse_nrst(Duration::from_millis(100))?);
    }

    // If the user specified a JTAG clock frequency, apply it now.
    if let Some(&freq) = matches.get_one::<u32>("freq") {
        jtag.set_clock(freq)?;
    }

    // If the user specified a JTAG scan chain length, apply it now.
    if let Some(&max_length) = matches.get_one("scan-chain-length") {
        jtag.set_max_length(max_length);
    }

    // If the user specified IR lengths, parse and save them.
    let ir_lens = matches
        .get_many("ir-lengths")
        .map(|lens| lens.copied().collect::<Vec<usize>>());

    // Scan the JTAG chain to detect all available TAPs.
    let chain = jtag.scan(ir_lens.as_deref())?;

    // At this point we can handle the 'scan' command.
    if matches.subcommand_name().unwrap() == "scan" {
        print_jtag_chain(&chain);
        return Ok(());
    }

    // If the user specified a TAP, we'll use it, but otherwise
    // attempt to find a single FPGA in the scan chain.
    let (tap_idx, idcode) = if let Some(&tap_idx) = matches.get_one("tap") {
        if let Some(idcode) = check_tap_idx(&chain, tap_idx) {
            log::debug!("Provided tap index is a 7-series device");
            (tap_idx, idcode)
        } else {
            print_jtag_chain(&chain);
            bail!("The provided tap index {tap_idx} does not have an 7-series IDCODE.");
        }
    } else if let Some((index, idcode)) = auto_tap_idx(&chain) {
        (index, idcode)
    } else {
        print_jtag_chain(&chain);
        bail!("Could not find an 7-series IDCODE in the JTAG chain.");
    };

    // Create a TAP instance, consuming the JTAG instance.
    let tap = jtag.into_tap(chain, tap_idx)?;

    let mut x7 = X7::new(tap, idcode);
    let idcode = x7.idcode();

    match matches.subcommand_name() {
        Some("dna") => {
            if !quiet { println!("Reading DNA...") };
            x7.dna()?;
        },
        Some("status") => {
            if !quiet { println!("Reading status...") };
            x7.status()?;
        },
        _ => panic!("Unhandled command."),
    }

    let t1 = t0.elapsed();
    if !quiet {
        println!("Finished in {}.{:02}s", t1.as_secs(), t1.subsec_millis()/10);
    }

    Ok(())
}

fn print_probe_list() {
    let probes = ProbeInfo::list();
    if probes.is_empty() {
        println!("No CMSIS-DAP probes found.");
    } else {
        println!("Found {} CMSIS-DAP probe{}:", probes.len(),
                 if probes.len() == 1 { "" } else { "s" });
        for probe in probes {
            println!("  {}", probe);
        }
    }
}

fn print_jtag_chain(chain: &JTAGChain) {
    println!("Detected JTAG chain, closest to TDO first:");
    let idcodes = chain.idcodes();
    let lines = chain.to_lines();
    for (idcode, line) in idcodes.iter().zip(lines.iter()) {
        if let Some(Some(x7)) = idcode.map(X7IDCODE::try_from_idcode) {
            println!(" - {} [{}]", line, x7.name());
        } else {
            println!(" - {}", line);
        }
    }
}
