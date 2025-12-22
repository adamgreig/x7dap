// Copyright 2025 Adam Greig
// Licensed under the Apache-2.0 and MIT licenses.
#![doc = include_str!("../README.md")]

use std::convert::{From, TryFrom};
use std::fmt;
use num_enum::{FromPrimitive, TryFromPrimitive};
use indicatif::{ProgressBar, ProgressStyle};
use jtagdap::jtag::{IDCODE, JTAGTAP, JTAGChain, Error as JTAGError};
use jtagdap::bitvec::{self, byte_to_bits, bytes_to_bits, bits_to_bytes, drain_u32, Error as BitvecError};

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
#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive)]
#[allow(non_camel_case_types)]
#[repr(u32)]
pub enum X7IDCODE {
    X7S6 = 0x3622093,
    X7S15 = 0x3620093,
    X7S25 = 0x37C4093,
    X7S50 = 0x362F093,
    X7S75 = 0x37C8093,
    X7S100 = 0x37c7093,
    X7Z10 = 0x3722093,
    X7Z20 = 0x3727093,
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
            "XC7S6"      => Some(X7IDCODE::X7S6),
            "XC7S15"     => Some(X7IDCODE::X7S15),
            "XC7S25"     => Some(X7IDCODE::X7S25),
            "XC7S50"     => Some(X7IDCODE::X7S50),
            "XC7S75"     => Some(X7IDCODE::X7S75),
            "XC7S100"    => Some(X7IDCODE::X7S100),
            "XC7Z10"     => Some(X7IDCODE::X7Z10),
            "XC7Z20"     => Some(X7IDCODE::X7Z20),
            _           => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            X7IDCODE::X7S6 => "XC7S6",
            X7IDCODE::X7S15 => "XC7S15",
            X7IDCODE::X7S25 => "XC7S25",
            X7IDCODE::X7S50 => "XC7S50",
            X7IDCODE::X7S75 => "XC7S75",
            X7IDCODE::X7S100 => "XC7S100",
            X7IDCODE::X7Z10 => "XC7Z010",
            X7IDCODE::X7Z20 => "XC7Z020",
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

/// All known 7-series JTAG instructions.
#[derive(Copy, Clone, Debug)]
#[allow(unused, non_camel_case_types, clippy::upper_case_acronyms)]
#[repr(u8)]
enum Command {
    USERCODE = 0b001000,
    IDCODE = 0b001001,
    HIGHZ_IO = 0b001010,
    CFG_OUT = 0b000100,
    CFG_IN = 0b000101,
    ISC_ENABLE = 0b010000,
    ISC_PROGRAM = 0b010001,
    XSC_PROGRAM_KEY = 0b010010,
    XSC_DNA = 0b010111,
    FUSE_DNA = 0b110010,
    ISC_NOOP = 0b010100,
    ISC_DISABLE = 0b010110,
    STATUS = 0b011111,
    BYPASS = 0b111111,
}

impl Command {
    pub fn bits(&self) -> Vec<bool> {
        jtagdap::bitvec::bytes_to_bits(&[*self as u8], 6).unwrap()
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

    pub fn dna(&mut self) -> Result<()> {
        self.command(Command::FUSE_DNA)?;
        let data = self.tap.read_dr(64)?;
        let dna = bitvec::bits_to_bytes(&data);
        log::info!("DNA: {:02X?}", dna);
        Ok(())
    }

    pub fn status(&mut self) -> Result<()> {
        self.tap.test_logic_reset()?;
        self.tap.run_test_idle(5)?;
        self.tap.write_ir(&Command::CFG_IN.bits())?;
        let mut bits = Vec::new();
        bitvec::append_u32(&mut bits, 0xaa99_5566u32.reverse_bits());
        bitvec::append_u32(&mut bits, 0x2000_0000u32.reverse_bits());
        bitvec::append_u32(&mut bits, 0x2800_e001u32.reverse_bits());
        bitvec::append_u32(&mut bits, 0x2000_0000u32.reverse_bits());
        bitvec::append_u32(&mut bits, 0x2000_0000u32.reverse_bits());
        self.tap.write_dr(&bits)?;
        self.tap.write_ir(&Command::CFG_OUT.bits())?;
        let status = bitvec::bits_to_bytes(&self.tap.read_dr(32)?);
        let status = u32::from_le_bytes([status[0], status[1], status[2], status[3]]);
        log::info!("Status: {:08X?}", status.reverse_bits());
        self.tap.test_logic_reset()?;
        Ok(())
    }

    /// Load a command into the IR.
    fn command(&mut self, command: Command) -> Result<()> {
        log::trace!("Loading command {:?}", command);
        Ok(self.tap.write_ir(&command.bits())?)
    }
}
