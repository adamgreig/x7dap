# X7DAP

[![crates.io](https://img.shields.io/crates/v/x7dap.svg)](https://crates.io/crates/x7dap)
[![docs.rs](https://docs.rs/x7dap/badge.svg)](https://docs.rs/x7dap)
![CI](https://github.com/adamgreig/x7dap/workflows/CI/badge.svg)

X7DAP allows you to program Xilinx 7-series FPGAs and SoCs using CMSIS-DAP
probes in JTAG mode.

This crate uses [jtagdap] to handle CMSIS-DAP and JTAG. Currently (Q)SPI flash
programming is not supported though it may be added at a future date.

For programming SPI flashes directly, for example when using iCE40 FPGAs, check
out [spidap], which uses the same libraries. For Lattice ECP5 FPGAs and
attached SPI flash, see [ecpdap].

[jtagdap]: https://github.com/adamgreig/jtagdap
[spidap]: https://github.com/adamgreig/spidap
[ecpdap]: https://github.com/adamgreig/ecpdap

## JTAG Clock Frequency

The default clock frequency is 1MHz, but in many situations higher frequencies
are possible and reduce operation time. It is also possible to require lower
speeds in situations with poor signal integrity.

Use `-f` or `--freq` to change, for example `-f 10M`.

## JTAG Scan Chains

FPGAs can be programmed on arbitrary length JTAG scan chains; you may need
to specify `--ir-lengths` and possibly specify a higher `--scan-chain-length`
depending on the other devices on your scan chain. Where possible the scan
chain is automatically detected.

## Pre-built Binaries

Pre-built binaries are available for Windows and Linux on the [Releases] page.
You must have [libusb] installed or available on your system, and you must
have permissions or drivers set up to access your CMSIS-DAP probe. See the
[drivers] page for information on setup.

[Releases]: https://github.com/adamgreig/x7dap/releases
[libusb]: https://libusb.info
[drivers]: https://github.com/adamgreig/x7dap/tree/master/drivers

## Building

* You must have a working Rust compiler installed.
  Visit [rustup.rs](https://rustup.rs) to install Rust.
* [libusb] is required to use the higher-speed CMSIS-DAPv2 protocol, where supported by your probe.
* You may need to set up drivers or permissions to access the USB device, see `drivers/` for details

To build and install for your user, without checking out the repository:

```sh
cargo install x7dap
```

Or, building locally after checking out this repository:

```sh
cargo build --release
```

You can either run the ecpdap executable directly from `target/release/x7dap`,
or you can install it for your user using `cargo install --path .`.

## Usage

Run `x7dap help` for detailed usage. Commonly used commands:

* `ecpdap probes`: List all detected CMSIS-DAP probes
* `ecpdap scan`: Scan the JTAG chain to detect ECP5 devices
* `ecpdap program bitstream.bit -f10M`: Program `bitstream.bit` to the ECP5 at 10MHz

## Licence

x7dap is licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
