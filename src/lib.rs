// Copyright 2025 Adam Greig
// Licensed under the Apache-2.0 and MIT licenses.
#![doc = include_str!("../README.md")]

use std::{fmt, time::Duration, convert::{From, TryFrom}, path::Path, fs::File, io::Read};
use num_enum::TryFromPrimitive;
use indicatif::{ProgressBar, ProgressStyle};
use jtagdap::jtag::{IDCODE, JTAGTAP, JTAGChain, Error as JTAGError};
use jtagdap::bitvec::{self, bytes_to_bits, bits_to_bytes, Error as BitvecError};

pub use jtagdap;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Device status register in incorrect state.")]
    BadStatus,
    #[error("Cannot access flash memory unless the device is the only TAP in the JTAG chain.")]
    NotOnlyTAP,
    #[error(
        "Bitstream file contains an IDCODE 0x{bitstream:08X} incompatible \
         with the detected device IDCODE 0x{jtag:08X}."
    )]
    IncompatibleIdcode { bitstream: u32, jtag: u32 },
    #[error("Could not remove VERIFY_IDCODE because parsing the bitstream failed")]
    RemoveIdcodeNoMetadata,
    #[error("SPI Flash error")]
    SPIFlash(#[from] spi_flash::Error),
    #[error("JTAG error")]
    JTAG(#[from] JTAGError),
    #[error("Bitvec error")]
    Bitvec(#[from] BitvecError),
    #[error("I/O error")]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// IDCODEs for all X7 device types.
///
/// IDCODEs are the same between C/A/Q part numbers (e.g. XC7Z030, XA7Z030, XQ7Z030).
///
/// Note first byte is the revision which may vary and so is 0 here.
#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive)]
#[allow(non_camel_case_types)]
#[repr(u32)]
pub enum X7IDCODE {
    X7S6        = 0x03622093,
    X7S15       = 0x03620093,
    X7S25       = 0x037C4093,
    X7S50       = 0x0362F093,
    X7S75       = 0x037C8093,
    X7S100      = 0x037c7093,
    X7A12T      = 0x037c3093,
    X7A15T      = 0x0362E093,
    X7A25T      = 0x037C2093,
    X7A35T      = 0x0362D093,
    X7A50T      = 0x0362C093,
    X7A75T      = 0x03632093,
    X7A100T     = 0x03631093,
    X7A200T     = 0x03636093,
    X7K70T      = 0x03647093,
    X7K160T     = 0x0364C093,
    X7K325T     = 0x03651093,
    X7K355T     = 0x03747093,
    X7K410T     = 0x03656093,
    X7K420T     = 0x03752093,
    X7K480T     = 0x03751093,
    X7V575T     = 0x03671093,
    X7VX330T    = 0x03667093,
    X7VX415T    = 0x03682093,
    X7VX485T    = 0x03687093,
    X7VX550T    = 0x03692093,
    X7VX690T    = 0x03691093,
    X7VX980T    = 0x03696093,
    X7VX1140T   = 0x036D5093,
    X7VH580T    = 0x036D9093,
    X7VH870T    = 0x036DB093,
    X7Z007S     = 0x03723093,
    X7Z012S     = 0x0373c093,
    X7Z014S     = 0x03728093,
    X7Z010      = 0x03722093,
    X7Z015      = 0x0373b093,
    X7Z020      = 0x03727093,
    X7Z030      = 0x0372c093,
    X7Z035      = 0x03732093,
    X7Z045      = 0x03731093,
    X7Z100      = 0x03736093,
}

impl From<X7IDCODE> for IDCODE {
    fn from(id: X7IDCODE) -> IDCODE {
        IDCODE(id as u32)
    }
}

impl From<&X7IDCODE> for IDCODE {
    fn from(id: &X7IDCODE) -> IDCODE {
        IDCODE(*id as u32)
    }
}

impl X7IDCODE {
    pub fn try_from_idcode(idcode: IDCODE) -> Option<Self> {
        Self::try_from(idcode.0 & 0x0FFF_FFFF).ok()
    }

    pub fn try_from_u32(idcode: u32) -> Option<Self> {
        Self::try_from_idcode(IDCODE(idcode))
    }

    pub fn try_from_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "X7S6" => Some(X7IDCODE::X7S6),
            "X7S15" => Some(X7IDCODE::X7S15),
            "X7S25" => Some(X7IDCODE::X7S25),
            "X7S50" => Some(X7IDCODE::X7S50),
            "X7S75" => Some(X7IDCODE::X7S75),
            "X7S100" => Some(X7IDCODE::X7S100),
            "X7A12T" => Some(X7IDCODE::X7A12T),
            "X7A15T" => Some(X7IDCODE::X7A15T),
            "X7A25T" => Some(X7IDCODE::X7A25T),
            "X7A35T" => Some(X7IDCODE::X7A35T),
            "X7A50T" => Some(X7IDCODE::X7A50T),
            "X7A75T" => Some(X7IDCODE::X7A75T),
            "X7A100T" => Some(X7IDCODE::X7A100T),
            "X7A200T" => Some(X7IDCODE::X7A200T),
            "X7K70T" => Some(X7IDCODE::X7K70T),
            "X7K160T" => Some(X7IDCODE::X7K160T),
            "X7K325T" => Some(X7IDCODE::X7K325T),
            "X7K355T" => Some(X7IDCODE::X7K355T),
            "X7K410T" => Some(X7IDCODE::X7K410T),
            "X7K420T" => Some(X7IDCODE::X7K420T),
            "X7K480T" => Some(X7IDCODE::X7K480T),
            "X7V575T" => Some(X7IDCODE::X7V575T),
            "X7VX330T" => Some(X7IDCODE::X7VX330T),
            "X7VX415T" => Some(X7IDCODE::X7VX415T),
            "X7VX485T" => Some(X7IDCODE::X7VX485T),
            "X7VX550T" => Some(X7IDCODE::X7VX550T),
            "X7VX690T" => Some(X7IDCODE::X7VX690T),
            "X7VX980T" => Some(X7IDCODE::X7VX980T),
            "X7VX1140T" => Some(X7IDCODE::X7VX1140T),
            "X7VH580T" => Some(X7IDCODE::X7VH580T),
            "X7VH870T" => Some(X7IDCODE::X7VH870T),
            "X7Z007S" => Some(X7IDCODE::X7Z007S),
            "X7Z012S" => Some(X7IDCODE::X7Z012S),
            "X7Z014S" => Some(X7IDCODE::X7Z014S),
            "X7Z010" => Some(X7IDCODE::X7Z010),
            "X7Z015" => Some(X7IDCODE::X7Z015),
            "X7Z020" => Some(X7IDCODE::X7Z020),
            "X7Z030" => Some(X7IDCODE::X7Z030),
            "X7Z035" => Some(X7IDCODE::X7Z035),
            "X7Z045" => Some(X7IDCODE::X7Z045),
            "X7Z100" => Some(X7IDCODE::X7Z100),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            X7IDCODE::X7S6 => "X7S6",
            X7IDCODE::X7S15 => "X7S15",
            X7IDCODE::X7S25 => "X7S25",
            X7IDCODE::X7S50 => "X7S50",
            X7IDCODE::X7S75 => "X7S75",
            X7IDCODE::X7S100 => "X7S100",
            X7IDCODE::X7A12T => "X7A12T",
            X7IDCODE::X7A15T => "X7A15T",
            X7IDCODE::X7A25T => "X7A25T",
            X7IDCODE::X7A35T => "X7A35T",
            X7IDCODE::X7A50T => "X7A50T",
            X7IDCODE::X7A75T => "X7A75T",
            X7IDCODE::X7A100T => "X7A100T",
            X7IDCODE::X7A200T => "X7A200T",
            X7IDCODE::X7K70T => "X7K70T",
            X7IDCODE::X7K160T => "X7K160T",
            X7IDCODE::X7K325T => "X7K325T",
            X7IDCODE::X7K355T => "X7K355T",
            X7IDCODE::X7K410T => "X7K410T",
            X7IDCODE::X7K420T => "X7K420T",
            X7IDCODE::X7K480T => "X7K480T",
            X7IDCODE::X7V575T => "X7V575T",
            X7IDCODE::X7VX330T => "X7VX330T",
            X7IDCODE::X7VX415T => "X7VX415T",
            X7IDCODE::X7VX485T => "X7VX485T",
            X7IDCODE::X7VX550T => "X7VX550T",
            X7IDCODE::X7VX690T => "X7VX690T",
            X7IDCODE::X7VX980T => "X7VX980T",
            X7IDCODE::X7VX1140T => "X7VX1140T",
            X7IDCODE::X7VH580T => "X7VH580T",
            X7IDCODE::X7VH870T => "X7VH870T",
            X7IDCODE::X7Z007S => "X7Z007S",
            X7IDCODE::X7Z012S => "X7Z012S",
            X7IDCODE::X7Z014S => "X7Z014S",
            X7IDCODE::X7Z010 => "X7Z010",
            X7IDCODE::X7Z015 => "X7Z015",
            X7IDCODE::X7Z020 => "X7Z020",
            X7IDCODE::X7Z030 => "X7Z030",
            X7IDCODE::X7Z035 => "X7Z035",
            X7IDCODE::X7Z045 => "X7Z045",
            X7IDCODE::X7Z100 => "X7Z100",
        }
    }

    /// Returns whether the provided IDCODE is considered compatible with
    /// this IDCODE.
    pub fn compatible(&self, other: X7IDCODE) -> bool {
        *self == other
    }

    /// Number of configuration bits per frame.
    ///
    /// Returns (pad_bits_before_frame, bits_per_frame, pad_bits_after_frame).
    pub fn config_bits_per_frame(&self) -> (usize, usize, usize) {
        (0, 0, 0)
    }
}

pub fn check_tap_idx(chain: &JTAGChain, index: usize) -> Option<X7IDCODE> {
    match chain.idcodes().iter().nth(index) {
        Some(Some(idcode)) => X7IDCODE::try_from_idcode(*idcode),
        _ => None,
    }
}

/// Attempt to discover a unique TAP index for a 7-series device in a JTAGChain.
pub fn auto_tap_idx(chain: &JTAGChain) -> Option<(usize, X7IDCODE)> {
    let x7_idxs: Vec<(usize, X7IDCODE)> = chain
        .idcodes()
        .iter()
        .enumerate()
        .filter_map(|(idx, id)| id.map(|id| (idx, id)))
        .filter_map(|(idx, id)| X7IDCODE::try_from_idcode(id).map(|id| (idx, id)))
        .collect();
    let len = x7_idxs.len();
    if len == 0 {
        log::info!("No 7-series device found in JTAG chain");
        None
    } else if len > 1 {
        log::info!("Multiple 7-series devices found in JTAG chain, specify one using --tap");
        None
    } else {
        let (index, idcode) = x7_idxs.first().unwrap();
        log::debug!("Automatically selecting device at TAP {}", index);
        Some((*index, *idcode))
    }
}

/// 7-series JTAG instructions.
#[derive(Copy, Clone, Debug)]
#[allow(unused, non_camel_case_types, clippy::upper_case_acronyms)]
#[repr(u8)]
enum Command {
    EXTEST = 0b100110,
    EXTEST_PULSE = 0b111100,
    EXTEST_TRAIN = 0b1111101,
    SAMPLE = 0b000001,
    USER1 = 0b000010,
    USER2 = 0b000011,
    USER3 = 0b100010,
    USER4 = 0b100011,
    CFG_OUT = 0b000100,
    CFG_IN = 0b000101,
    USERCODE = 0b001000,
    IDCODE = 0b001001,
    HIGHZ_IO = 0b001010,
    JPROGRAM = 0b001011,
    JSTART = 0b001100,
    JSHUTDOWN = 0b001101,
    XADC_DRP = 0b110111,
    ISC_ENABLE = 0b010000,
    ISC_PROGRAM = 0b010001,
    XSC_PROGRAM_KEY = 0b010010,
    XSC_DNA = 0b010111,
    FUSE_DNA = 0b110010,
    ISC_NOOP = 0b010100,
    ISC_DISABLE = 0b010110,
    BYPASS = 0b111111,
}

impl Command {
    pub fn bits(&self) -> Vec<bool> {
        jtagdap::bitvec::bytes_to_bits(&[*self as u8], 6).unwrap()
    }
}

/// Configuration status register.
#[derive(Copy, Clone)]
pub struct Status(u32);

impl Status {
    pub fn new(word: u32) -> Self {
        Self(word)
    }

    pub fn startup_state(&self) -> u8       { ((self.0 >> 18) & 0b111) as u8 }
    pub fn xadc_overtemp(&self) -> bool     { self.bit(17) }
    pub fn dec_error(&self) -> bool         { self.bit(16) }
    pub fn id_error(&self) -> bool          { self.bit(15) }
    pub fn done(&self) -> bool              { self.bit(14) }
    pub fn release_done(&self) -> bool      { self.bit(13) }
    pub fn init_b(&self) -> bool            { self.bit(12) }
    pub fn init_complete(&self) -> bool     { self.bit(11) }
    pub fn mode(&self) -> u8                { ((self.0 >> 8) & 0b111) as u8 }
    pub fn ghigh_b(&self) -> bool           { self.bit(7) }
    pub fn gwe(&self) -> bool               { self.bit(6) }
    pub fn gts_cfg_b(&self) -> bool         { self.bit(5) }
    pub fn eos(&self) -> bool               { self.bit(4) }
    pub fn dci_match(&self) -> bool         { self.bit(3) }
    pub fn mmcm_lock(&self) -> bool         { self.bit(2) }
    pub fn part_secured(&self) -> bool      { self.bit(1) }
    pub fn crc_error(&self) -> bool         { self.bit(0) }

    fn bit(&self, offset: usize) -> bool {
        (self.0 >> offset) & 1 == 1
    }
}

impl fmt::Debug for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            "Status: {:08X}
  Startup state: 0b{:03b}
  XADC overtemp: {}
  Decrypt error: {}
  ID error: {}
  DONE: {}
  Release DONE: {}
  INIT_B: {}
  INIT complete: {}
  Mode: 0b{:03b}
  GHIGH_B: {}
  Global write enable: {}
  Global tri-state: {}
  End of startup: {}
  DCI match: {}
  MMCM lock: {}
  Secured: {}
  CRC error: {}",
            self.0, self.startup_state(), self.xadc_overtemp(), self.dec_error(), self.id_error(),
            self.done(), self.release_done(), self.init_b(), self.init_complete(), self.mode(),
            self.ghigh_b(), self.gwe(), self.gts_cfg_b(), self.eos(), self.dci_match(),
            self.mmcm_lock(), self.part_secured(), self.crc_error()))
    }
}

pub struct X7 {
    tap: JTAGTAP,
    idcode: X7IDCODE,
}

impl X7 {
    pub fn new(tap: JTAGTAP, idcode: X7IDCODE) -> Self {
        X7 { tap, idcode }
    }

    pub fn idcode(&self) -> X7IDCODE {
        self.idcode
    }

    /// Read full 64-bit device DNA.
    pub fn dna(&mut self) -> Result<Vec<u8>> {
        self.command(Command::FUSE_DNA)?;
        let data = self.tap.read_dr(64)?;
        let dna = bits_to_bytes(&data);
        log::info!("Read DNA: {:02X?}", dna);
        Ok(dna)
    }

    /// Read STATUS register content.
    pub fn status(&mut self) -> Result<Status> {
        self.tap.test_logic_reset()?;
        self.tap.run_test_idle(5)?;
        self.command(Command::CFG_IN)?;
        let mut bits = Vec::new();
        bitvec::append_u32(&mut bits, 0xaa99_5566u32.reverse_bits());
        bitvec::append_u32(&mut bits, 0x2000_0000u32.reverse_bits());
        bitvec::append_u32(&mut bits, 0x2800_e001u32.reverse_bits());
        bitvec::append_u32(&mut bits, 0x2000_0000u32.reverse_bits());
        bitvec::append_u32(&mut bits, 0x2000_0000u32.reverse_bits());
        self.tap.write_dr(&bits)?;
        self.command(Command::CFG_OUT)?;
        let status = bits_to_bytes(&self.tap.read_dr(32)?);
        let status = u32::from_le_bytes([status[0], status[1], status[2], status[3]]);
        let status = Status::new(status.reverse_bits());
        log::debug!("{:?}", status);
        self.tap.test_logic_reset()?;
        Ok(status)
    }

    /// Program a bitstream to SRAM.
    ///
    /// The FPGA is reset and begins running the new bitstream after programming.
    pub fn program(&mut self, data: &[u8]) -> Result<()> {
        self.program_cb(data, |_| {})
    }

    /// Program a bitstream to SRAM, with a progress bar.
    ///
    /// The FPGA is reset and begins running the new bitstream after programming.
    pub fn program_progress(&mut self, data: &[u8]) -> Result<()> {
        const DATA_PROGRESS_TPL: &str =
            " {msg} [{bar:40.cyan/black}] {bytes}/{total_bytes} ({bytes_per_sec}; {eta_precise})";
        const DATA_FINISHED_TPL: &str =
            " {msg} [{bar:40.green/black}] {bytes}/{total_bytes} ({bytes_per_sec}; {eta_precise})";
        const DATA_PROGRESS_CHARS: &str = "━╸━";
        let pb = ProgressBar::new(data.len() as u64).with_style(
            ProgressStyle::with_template(DATA_PROGRESS_TPL)
                .unwrap()
                .progress_chars(DATA_PROGRESS_CHARS));
        pb.set_message("Programming");
        pb.set_position(0);

        self.program_cb(data, |n| pb.set_position(n as u64))?;

        pb.set_style(ProgressStyle::with_template(DATA_FINISHED_TPL)
            .unwrap()
            .progress_chars(DATA_PROGRESS_CHARS)
        );

        pb.finish();
        Ok(())
    }

    /// Program a bitstream to SRAM, calling `cb` with the number of bytes programmed so far.
    ///
    /// The FPGA is reset and begins running the new bitstream after programming.
    pub fn program_cb<F: Fn(usize)>(&mut self, data: &[u8], cb: F) -> Result<()> {
        // Reset FPGA and wait 10ms.
        self.check_ready_to_program()?;
        self.tap.test_logic_reset()?;
        self.command(Command::JPROGRAM)?;
        self.tap.run_test_idle(1)?;
        std::thread::sleep(Duration::from_millis(20));

        // Enter configuration mode.
        self.tap.test_logic_reset()?;
        self.command(Command::CFG_IN)?;

        // Load in entire bitstream.
        // We need to send the MSb of the first byte first and finish on the LSb of the last byte,
        // so since bytes_to_bits is LSb-first, we reverse the bit order of each byte.
        let data: Vec<u8> = data.iter().map(|x| x.reverse_bits()).collect();
        let bits = bytes_to_bits(&data, data.len() * 8)?;

        // Write bitstream, passing the callback through.
        self.tap.write_dr_cb(&bits, |n| cb(n / 8))?;

        // Return to Run-Test/Idle to complete programming.
        self.tap.run_test_idle(1)?;

        // Begin startup sequence.
        self.command(Command::JSTART)?;
        self.tap.run_test_idle(2000)?;
        self.tap.test_logic_reset()?;

        // Check programming was OK.
        self.check_programmed_ok()?;
        self.tap.test_logic_reset()?;

        Ok(())
    }

    pub fn jprogram(&mut self) -> Result<()> {
        self.command(Command::JPROGRAM)?;
        self.tap.run_test_idle(2000)?;
        self.tap.test_logic_reset()?;
        Ok(())
    }

    fn check_ready_to_program(&mut self) -> Result<()> {
        log::debug!("Checking status before programming...");
        let status = self.status()?;
        if !status.init_complete() {
            log::error!("FPGA init not complete");
            return Err(Error::BadStatus);
        }
        if !status.init_b() {
            log::error!("FPGA INIT_B still low");
            return Err(Error::BadStatus);
        }
        Ok(())
    }

    fn check_programmed_ok(&mut self) -> Result<()> {
        log::debug!("Checking status after programming...");
        let status = self.status()?;
        if !status.init_complete() {
            log::error!("Init not complete");
            return Err(Error::BadStatus);
        }
        if !status.init_b() {
            log::error!("INIT_B still low");
            return Err(Error::BadStatus);
        }
        if !status.done() {
            log::error!("DONE still low");
            return Err(Error::BadStatus);
        }
        if !status.release_done() {
            log::error!("DONE not released");
            return Err(Error::BadStatus);
        }
        if status.dec_error() {
            log::error!("Decrypt error");
            return Err(Error::BadStatus);
        }
        if status.id_error() {
            log::error!("ID error");
            return Err(Error::BadStatus);
        }
        if status.crc_error() {
            log::error!("CRC error");
            return Err(Error::BadStatus);
        }
        Ok(())
    }

    /// Load a command into the IR.
    fn command(&mut self, command: Command) -> Result<()> {
        log::trace!("Loading command {:?}", command);
        Ok(self.tap.write_ir(&command.bits())?)
    }
}

pub struct Bitstream {
    data: Vec<u8>,
}

impl Bitstream {
    /// Open a bitstream from the provided path.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = File::open(path)?;
        Self::from_file(&mut file)
    }

    /// Open a bitstream from the provided open `File`.
    pub fn from_file(file: &mut File) -> Result<Self> {
        let mut data = if let Ok(metadata) = file.metadata() {
            Vec::with_capacity(metadata.len() as usize)
        } else {
            Vec::new()
        };
        file.read_to_end(&mut data)?;
        Ok(Self::new(data))
    }

    /// Load a bitstream from the provided raw bitstream data.
    pub fn from_data(data: &[u8]) -> Self {
        Self::new(data.to_owned())
    }

    /// Load a bitstream directly from a `Vec<u8>`.
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Get the underlying bitstream data.
    pub fn data(&self) -> &[u8] {
        &self.data[..]
    }
}
