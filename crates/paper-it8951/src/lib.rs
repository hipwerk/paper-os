//! Portable, fail-closed IT8951 control protocol.
//!
//! Linux SPI/GPIO framing belongs in a [`Transport`] implementation. This crate
//! owns controller commands, probed identity, VCOM typing, and verified LUT
//! capability mapping.

#![no_std]

use core::fmt;

use paper_display::{
    DisplayCapabilities, PixelFormat, Size, UpdateConstraints, UpdateProfile, Waveform,
};

const COMMAND_SYSTEM_RUN: u16 = 0x0001;
const COMMAND_STANDBY: u16 = 0x0002;
const COMMAND_SLEEP: u16 = 0x0003;
const COMMAND_GET_DEVICE_INFO: u16 = 0x0302;
const COMMAND_VCOM: u16 = 0x0039;
const DEVICE_INFO_WORDS: usize = 20;

const QUALITY_PROFILES: &[UpdateProfile] = &[
    UpdateProfile::new(
        PixelFormat::Gray4,
        Waveform::Initialize,
        false,
        UpdateConstraints::UNRESTRICTED,
    ),
    UpdateProfile::new(
        PixelFormat::Gray8,
        Waveform::Grayscale,
        true,
        UpdateConstraints::UNRESTRICTED,
    ),
    UpdateProfile::new(
        PixelFormat::Gray4,
        Waveform::Grayscale,
        true,
        UpdateConstraints::UNRESTRICTED,
    ),
    UpdateProfile::new(
        PixelFormat::Gray2,
        Waveform::Grayscale,
        true,
        UpdateConstraints::UNRESTRICTED,
    ),
    UpdateProfile::new(
        PixelFormat::Monochrome1,
        Waveform::Grayscale,
        true,
        UpdateConstraints::UNRESTRICTED,
    ),
];
const FAST_ALIGNED_PROFILES: &[UpdateProfile] = &[
    QUALITY_PROFILES[0],
    QUALITY_PROFILES[1],
    QUALITY_PROFILES[2],
    QUALITY_PROFILES[3],
    QUALITY_PROFILES[4],
    UpdateProfile::new(
        PixelFormat::Monochrome1,
        Waveform::FastMonochrome,
        true,
        UpdateConstraints::new(32, 1, 32, 1).expect("IT8951 fast-profile alignments are non-zero"),
    ),
];
const FAST_UNRESTRICTED_PROFILES: &[UpdateProfile] = &[
    QUALITY_PROFILES[0],
    QUALITY_PROFILES[1],
    QUALITY_PROFILES[2],
    QUALITY_PROFILES[3],
    QUALITY_PROFILES[4],
    UpdateProfile::new(
        PixelFormat::Monochrome1,
        Waveform::FastMonochrome,
        true,
        UpdateConstraints::UNRESTRICTED,
    ),
];

/// The positive magnitude of the negative panel VCOM printed on its FPC.
///
/// ```
/// use paper_it8951::VcomMillivolts;
///
/// assert_eq!(VcomMillivolts::new(1_500).unwrap().get(), 1_500);
/// assert!(VcomMillivolts::new(0).is_none());
/// ```
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct VcomMillivolts(u16);

impl VcomMillivolts {
    /// Creates a plausible non-zero VCOM magnitude up to 5 V.
    pub const fn new(millivolts: u16) -> Option<Self> {
        if millivolts > 0 && millivolts <= 5_000 {
            Some(Self(millivolts))
        } else {
            None
        }
    }

    /// Returns the positive millivolt magnitude.
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// A verified LUT firmware family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LutFamily {
    /// Six-inch M641 firmware; A2 mode 4 with 32-pixel X/width alignment.
    M641,
    /// Six-inch M841 TFAB512 firmware; A2 mode 6 with 32-pixel alignment.
    M841Tfab512,
    /// Nine-inch M841 firmware; A2 mode 6.
    M841,
    /// 7.8-inch M841 TFA2812 firmware; A2 mode 6.
    M841Tfa2812,
    /// 10.3-inch M841 TFA5210 firmware; A2 mode 6.
    M841Tfa5210,
    /// Unrecognized firmware. Fast refresh is not advertised.
    Unknown,
}

impl LutFamily {
    /// Returns the controller A2 mode only for allowlisted firmware.
    pub const fn fast_monochrome_mode(self) -> Option<u16> {
        match self {
            Self::M641 => Some(4),
            Self::M841Tfab512 | Self::M841 | Self::M841Tfa2812 | Self::M841Tfa5210 => Some(6),
            Self::Unknown => None,
        }
    }

    const fn requires_four_byte_alignment(self) -> bool {
        matches!(self, Self::M641 | Self::M841Tfab512)
    }
}

/// Device information read directly from the controller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceInfo {
    /// Native panel dimensions.
    pub panel_size: Size,
    /// Controller image-buffer base address.
    pub image_buffer_address: u32,
    /// Raw, potentially NUL-terminated firmware version bytes.
    pub firmware_version: [u8; 16],
    /// Raw, potentially NUL-terminated LUT version bytes.
    pub lut_version: [u8; 16],
}

impl DeviceInfo {
    fn from_words(words: &[u16; DEVICE_INFO_WORDS]) -> Self {
        let mut firmware_version = [0; 16];
        let mut lut_version = [0; 16];
        words_to_bytes(&words[4..12], &mut firmware_version);
        words_to_bytes(&words[12..20], &mut lut_version);

        Self {
            panel_size: Size::new(u32::from(words[0]), u32::from(words[1])),
            image_buffer_address: u32::from(words[2]) | (u32::from(words[3]) << 16),
            firmware_version,
            lut_version,
        }
    }

    /// Classifies the probed LUT without guessing for unknown strings.
    pub fn lut_family(self) -> LutFamily {
        if version_equals(&self.lut_version, b"M641") {
            LutFamily::M641
        } else if version_equals(&self.lut_version, b"M841_TFAB512") {
            LutFamily::M841Tfab512
        } else if version_equals(&self.lut_version, b"M841") {
            LutFamily::M841
        } else if version_equals(&self.lut_version, b"M841_TFA2812") {
            LutFamily::M841Tfa2812
        } else if version_equals(&self.lut_version, b"M841_TFA5210") {
            LutFamily::M841Tfa5210
        } else {
            LutFamily::Unknown
        }
    }

    /// Builds conservative display capabilities from the probed LUT.
    pub fn capabilities(self) -> DisplayCapabilities {
        let family = self.lut_family();
        DisplayCapabilities {
            native_size: self.panel_size,
            update_profiles: if family.requires_four_byte_alignment() {
                FAST_ALIGNED_PROFILES
            } else if family.fast_monochrome_mode().is_some() {
                FAST_UNRESTRICTED_PROFILES
            } else {
                QUALITY_PROFILES
            },
        }
    }

    /// Returns the allowlisted controller A2 mode, if known.
    pub fn fast_monochrome_mode(self) -> Option<u16> {
        self.lut_family().fast_monochrome_mode()
    }
}

/// Identity and VCOM observed during a non-mutating controller probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProbeReport {
    /// Probed controller and panel identity.
    pub device_info: DeviceInfo,
    /// VCOM currently configured in the controller.
    pub current_vcom: VcomMillivolts,
}

fn version_equals(version: &[u8; 16], expected: &[u8]) -> bool {
    let length = version
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(version.len());
    &version[..length] == expected
}

fn words_to_bytes(words: &[u16], bytes: &mut [u8]) {
    for (word, pair) in words.iter().zip(bytes.chunks_exact_mut(2)) {
        let [first, second] = word.to_le_bytes();
        pair[0] = first;
        pair[1] = second;
    }
}

/// Lowest common protocol operations, implemented by Linux SPI or an MCU HAL.
///
/// Each method represents one complete IT8951 transaction, including its
/// preamble, chip-select lifetime, and ready-pin synchronization.
pub trait Transport {
    /// Transport-specific error.
    type Error;

    /// Pulses reset and waits until the controller becomes ready.
    fn reset(&mut self) -> Result<(), Self::Error>;

    /// Sends one host-order command word.
    fn command(&mut self, command: u16) -> Result<(), Self::Error>;

    /// Sends one host-order data word.
    fn write_word(&mut self, word: u16) -> Result<(), Self::Error>;

    /// Reads controller words into host-order `u16` values.
    ///
    /// IT8951 SPI transfers the most-significant byte first. The transport must
    /// assemble those bytes into the numeric word before returning it. Device
    /// version strings are then decoded from the little-endian in-memory word
    /// representation mandated by the controller's device-info structure.
    fn read_words(&mut self, words: &mut [u16]) -> Result<(), Self::Error>;
}

/// IT8951 control failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error<E> {
    /// Underlying SPI/GPIO transport failure.
    Transport(E),
    /// The controller returned an empty panel size.
    InvalidDeviceInfo,
    /// The controller returned a zero or implausible VCOM magnitude.
    InvalidVcomResponse,
    /// A VCOM write completed at the transport layer but did not persist.
    VcomMismatch {
        /// Requested positive magnitude.
        requested: VcomMillivolts,
        /// Value observed immediately after the write.
        observed: VcomMillivolts,
    },
}

impl<E: fmt::Display> fmt::Display for Error<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(error) => write!(formatter, "IT8951 transport error: {error}"),
            Self::InvalidDeviceInfo => formatter.write_str("IT8951 returned an invalid panel size"),
            Self::InvalidVcomResponse => formatter.write_str("IT8951 returned an invalid VCOM"),
            Self::VcomMismatch {
                requested,
                observed,
            } => write!(
                formatter,
                "IT8951 VCOM verification failed: requested {} mV, observed {} mV",
                requested.get(),
                observed.get()
            ),
        }
    }
}

/// Safe high-level control commands over an IT8951 transport.
pub struct Controller<T> {
    transport: T,
}

impl<T: Transport> Controller<T> {
    /// Wraps a transport without touching hardware.
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }

    /// Resets, wakes, and probes identity and VCOM without changing VCOM.
    pub fn probe(&mut self) -> Result<ProbeReport, Error<T::Error>> {
        self.transport.reset().map_err(Error::Transport)?;
        self.wake()?;
        Ok(ProbeReport {
            device_info: self.device_info()?,
            current_vcom: self.vcom()?,
        })
    }

    /// Reads controller identity without resetting or changing configuration.
    pub fn device_info(&mut self) -> Result<DeviceInfo, Error<T::Error>> {
        self.transport
            .command(COMMAND_GET_DEVICE_INFO)
            .map_err(Error::Transport)?;
        let mut words = [0; DEVICE_INFO_WORDS];
        self.transport
            .read_words(&mut words)
            .map_err(Error::Transport)?;
        let info = DeviceInfo::from_words(&words);
        if info.panel_size.is_empty() {
            return Err(Error::InvalidDeviceInfo);
        }
        Ok(info)
    }

    /// Reads the current positive VCOM magnitude.
    pub fn vcom(&mut self) -> Result<VcomMillivolts, Error<T::Error>> {
        self.transport
            .command(COMMAND_VCOM)
            .map_err(Error::Transport)?;
        self.transport.write_word(0).map_err(Error::Transport)?;
        let mut response = [0];
        self.transport
            .read_words(&mut response)
            .map_err(Error::Transport)?;
        VcomMillivolts::new(response[0]).ok_or(Error::InvalidVcomResponse)
    }

    /// Explicitly writes VCOM and verifies matching controller readback.
    ///
    /// This persistently changes a panel-health-sensitive controller setting.
    /// Callers must verify the value from the exact panel FPC and obtain operator
    /// authorization before invoking it.
    pub fn set_vcom(&mut self, vcom: VcomMillivolts) -> Result<(), Error<T::Error>> {
        self.transport
            .command(COMMAND_VCOM)
            .map_err(Error::Transport)?;
        self.transport.write_word(1).map_err(Error::Transport)?;
        self.transport
            .write_word(vcom.get())
            .map_err(Error::Transport)?;
        let observed = self.vcom()?;
        if observed != vcom {
            return Err(Error::VcomMismatch {
                requested: vcom,
                observed,
            });
        }
        Ok(())
    }

    /// Puts the controller in system-run state.
    pub fn wake(&mut self) -> Result<(), Error<T::Error>> {
        self.transport
            .command(COMMAND_SYSTEM_RUN)
            .map_err(Error::Transport)
    }

    /// Puts the controller in standby state.
    pub fn standby(&mut self) -> Result<(), Error<T::Error>> {
        self.transport
            .command(COMMAND_STANDBY)
            .map_err(Error::Transport)
    }

    /// Puts the controller in sleep state.
    pub fn sleep(&mut self) -> Result<(), Error<T::Error>> {
        self.transport
            .command(COMMAND_SLEEP)
            .map_err(Error::Transport)
    }

    /// Returns the owned transport.
    pub fn into_transport(self) -> T {
        self.transport
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec;
    use std::vec::Vec;

    use paper_display::{PixelFormat, Size, UpdateConstraints, Waveform};

    use super::{
        COMMAND_GET_DEVICE_INFO, COMMAND_SYSTEM_RUN, COMMAND_VCOM, Controller, DeviceInfo, Error,
        LutFamily, Transport, VcomMillivolts,
    };

    #[derive(Default)]
    struct FakeTransport {
        operations: Vec<Operation>,
        reads: Vec<u16>,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Operation {
        Reset,
        Command(u16),
        Write(u16),
    }

    impl Transport for FakeTransport {
        type Error = ();

        fn reset(&mut self) -> Result<(), Self::Error> {
            self.operations.push(Operation::Reset);
            Ok(())
        }

        fn command(&mut self, command: u16) -> Result<(), Self::Error> {
            self.operations.push(Operation::Command(command));
            Ok(())
        }

        fn write_word(&mut self, word: u16) -> Result<(), Self::Error> {
            self.operations.push(Operation::Write(word));
            Ok(())
        }

        fn read_words(&mut self, words: &mut [u16]) -> Result<(), Self::Error> {
            words.copy_from_slice(&self.reads[..words.len()]);
            self.reads.drain(..words.len());
            Ok(())
        }
    }

    fn words_for_lut(lut: &[u8]) -> Vec<u16> {
        let mut bytes = [0_u8; 16];
        bytes[..lut.len()].copy_from_slice(lut);
        bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect()
    }

    fn device_info(lut: &[u8]) -> DeviceInfo {
        let mut lut_version = [0_u8; 16];
        lut_version[..lut.len()].copy_from_slice(lut);
        DeviceInfo {
            panel_size: Size::new(1448, 1072),
            image_buffer_address: 0x1234_5678,
            firmware_version: [0; 16],
            lut_version,
        }
    }

    #[test]
    fn probe_reads_vcom_without_writing_it() {
        let mut reads = vec![1448, 1072, 0x1234, 0x5678, 0x3646, 0, 0, 0, 0, 0, 0, 0];
        reads.extend(words_for_lut(b"M641"));
        reads.push(1_400);
        let transport = FakeTransport {
            operations: Vec::new(),
            reads,
        };
        let mut controller = Controller::new(transport);

        let report = controller.probe().unwrap();
        let transport = controller.into_transport();

        assert_eq!(report.device_info.panel_size, Size::new(1448, 1072));
        assert_eq!(report.current_vcom.get(), 1_400);
        assert_eq!(
            transport.operations,
            vec![
                Operation::Reset,
                Operation::Command(COMMAND_SYSTEM_RUN),
                Operation::Command(COMMAND_GET_DEVICE_INFO),
                Operation::Command(COMMAND_VCOM),
                Operation::Write(0),
            ]
        );
    }

    #[test]
    fn literal_controller_words_decode_lut_byte_order() {
        let mut words = [0_u16; super::DEVICE_INFO_WORDS];
        words[0] = 1448;
        words[1] = 1072;
        // The transport returns host-order words assembled from MSB-first SPI.
        // Casting those words to bytes on Waveshare's little-endian Pi yields
        // "M641", so this fixture intentionally does not call words_for_lut.
        words[12] = 0x364d;
        words[13] = 0x3134;

        assert_eq!(DeviceInfo::from_words(&words).lut_family(), LutFamily::M641);
    }

    #[test]
    fn setting_vcom_is_a_separate_explicit_operation() {
        let transport = FakeTransport {
            operations: Vec::new(),
            reads: vec![1_500],
        };
        let mut controller = Controller::new(transport);
        controller
            .set_vcom(VcomMillivolts::new(1_500).unwrap())
            .unwrap();
        assert_eq!(
            controller.into_transport().operations,
            vec![
                Operation::Command(COMMAND_VCOM),
                Operation::Write(1),
                Operation::Write(1_500),
                Operation::Command(COMMAND_VCOM),
                Operation::Write(0),
            ]
        );
    }

    #[test]
    fn setting_vcom_fails_when_readback_does_not_match() {
        let transport = FakeTransport {
            operations: Vec::new(),
            reads: vec![1_400],
        };
        let mut controller = Controller::new(transport);
        let requested = VcomMillivolts::new(1_500).unwrap();

        assert_eq!(
            controller.set_vcom(requested),
            Err(Error::VcomMismatch {
                requested,
                observed: VcomMillivolts::new(1_400).unwrap(),
            })
        );
    }

    #[test]
    fn six_inch_lut_families_both_require_alignment() {
        for (name, family, mode) in [
            (b"M641".as_slice(), LutFamily::M641, 4),
            (b"M841_TFAB512".as_slice(), LutFamily::M841Tfab512, 6),
        ] {
            let info = device_info(name);
            assert_eq!(info.lut_family(), family);
            assert_eq!(info.fast_monochrome_mode(), Some(mode));
            assert_eq!(
                info.capabilities()
                    .profile(PixelFormat::Monochrome1, Waveform::FastMonochrome)
                    .unwrap()
                    .constraints(),
                UpdateConstraints::new(32, 1, 32, 1).unwrap()
            );
        }
    }

    #[test]
    fn unknown_lut_does_not_advertise_or_guess_fast_mode() {
        let info = device_info(b"FUTURE_UNKNOWN");
        assert_eq!(info.lut_family(), LutFamily::Unknown);
        assert_eq!(info.fast_monochrome_mode(), None);
        assert!(
            info.capabilities()
                .profile(PixelFormat::Monochrome1, Waveform::FastMonochrome)
                .is_none()
        );
    }

    #[test]
    fn vcom_must_be_explicit_and_plausible() {
        assert_eq!(VcomMillivolts::new(0), None);
        assert_eq!(VcomMillivolts::new(5_001), None);
        assert_eq!(VcomMillivolts::new(1_500).unwrap().get(), 1_500);
    }
}
