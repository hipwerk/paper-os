#![no_std]

use core::fmt;

use paper_display::{DisplayCapabilities, PixelFormat, Size, UpdateConstraints, Waveform};

const COMMAND_SYSTEM_RUN: u16 = 0x0001;
const COMMAND_STANDBY: u16 = 0x0002;
const COMMAND_SLEEP: u16 = 0x0003;
const COMMAND_GET_DEVICE_INFO: u16 = 0x0302;
const COMMAND_VCOM: u16 = 0x0039;
const DEVICE_INFO_WORDS: usize = 20;

const FORMATS: &[PixelFormat] = &[
    PixelFormat::Monochrome1,
    PixelFormat::Gray2,
    PixelFormat::Gray4,
    PixelFormat::Gray8,
];
const WAVEFORMS: &[Waveform] = &[
    Waveform::Initialize,
    Waveform::Grayscale,
    Waveform::FastMonochrome,
];

/// The positive magnitude of the negative panel VCOM printed on its FPC.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct VcomMillivolts(u16);

impl VcomMillivolts {
    pub const fn new(millivolts: u16) -> Option<Self> {
        if millivolts > 0 && millivolts <= 5_000 {
            Some(Self(millivolts))
        } else {
            None
        }
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Device information read directly from the controller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceInfo {
    pub panel_size: Size,
    pub image_buffer_address: u32,
    pub firmware_version: [u8; 16],
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

    pub fn capabilities(self) -> DisplayCapabilities {
        let m641 = self.lut_version.starts_with(b"M641");
        DisplayCapabilities {
            native_size: self.panel_size,
            supported_formats: FORMATS,
            supported_waveforms: WAVEFORMS,
            partial_updates: true,
            fast_monochrome_constraints: if m641 {
                UpdateConstraints::new(32, 1, 32, 1)
            } else {
                UpdateConstraints::UNRESTRICTED
            },
        }
    }

    /// Maps semantic fast-monochrome intent to the LUT mode exposed by firmware.
    pub fn fast_monochrome_mode(self) -> u16 {
        if self.lut_version.starts_with(b"M641") {
            4
        } else {
            6
        }
    }
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
/// Each command/data method represents one IT8951 transaction, including its
/// preamble and ready-pin synchronization.
pub trait Transport {
    type Error;

    fn reset(&mut self) -> Result<(), Self::Error>;
    fn command(&mut self, command: u16) -> Result<(), Self::Error>;
    fn write_word(&mut self, word: u16) -> Result<(), Self::Error>;
    fn read_words(&mut self, words: &mut [u16]) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error<E> {
    Transport(E),
    InvalidDeviceInfo,
}

impl<E: fmt::Display> fmt::Display for Error<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(error) => write!(formatter, "IT8951 transport error: {error}"),
            Self::InvalidDeviceInfo => formatter.write_str("IT8951 returned an invalid panel size"),
        }
    }
}

/// Safe high-level control commands over an IT8951 transport.
pub struct Controller<T> {
    transport: T,
}

impl<T: Transport> Controller<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn initialize(
        &mut self,
        panel_vcom: VcomMillivolts,
    ) -> Result<DeviceInfo, Error<T::Error>> {
        self.transport.reset().map_err(Error::Transport)?;
        self.wake()?;
        let info = self.device_info()?;

        if self.vcom()? != panel_vcom {
            self.set_vcom(panel_vcom)?;
        }
        Ok(info)
    }

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

    pub fn vcom(&mut self) -> Result<VcomMillivolts, Error<T::Error>> {
        self.transport
            .command(COMMAND_VCOM)
            .map_err(Error::Transport)?;
        self.transport.write_word(0).map_err(Error::Transport)?;
        let mut response = [0];
        self.transport
            .read_words(&mut response)
            .map_err(Error::Transport)?;
        VcomMillivolts::new(response[0]).ok_or(Error::InvalidDeviceInfo)
    }

    pub fn set_vcom(&mut self, vcom: VcomMillivolts) -> Result<(), Error<T::Error>> {
        self.transport
            .command(COMMAND_VCOM)
            .map_err(Error::Transport)?;
        self.transport.write_word(1).map_err(Error::Transport)?;
        self.transport
            .write_word(vcom.get())
            .map_err(Error::Transport)
    }

    pub fn wake(&mut self) -> Result<(), Error<T::Error>> {
        self.transport
            .command(COMMAND_SYSTEM_RUN)
            .map_err(Error::Transport)
    }

    pub fn standby(&mut self) -> Result<(), Error<T::Error>> {
        self.transport
            .command(COMMAND_STANDBY)
            .map_err(Error::Transport)
    }

    pub fn sleep(&mut self) -> Result<(), Error<T::Error>> {
        self.transport
            .command(COMMAND_SLEEP)
            .map_err(Error::Transport)
    }

    pub fn into_transport(self) -> T {
        self.transport
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec;
    use std::vec::Vec;

    use paper_display::{Size, UpdateConstraints};

    use super::{
        COMMAND_GET_DEVICE_INFO, COMMAND_SYSTEM_RUN, COMMAND_VCOM, Controller, Transport,
        VcomMillivolts,
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

    #[test]
    fn initialization_probes_before_setting_vcom() {
        let mut reads = vec![
            1448, 1072, 0x1234, 0x5678, 0x3646, 0, 0, 0, 0, 0, 0, 0, 0x364d, 0x3134, 0, 0, 0, 0, 0,
            0,
        ];
        reads.push(1_400);
        let transport = FakeTransport {
            operations: Vec::new(),
            reads,
        };
        let mut controller = Controller::new(transport);

        let info = controller
            .initialize(VcomMillivolts::new(1_500).unwrap())
            .unwrap();
        let transport = controller.into_transport();

        assert_eq!(info.panel_size, Size::new(1448, 1072));
        assert_eq!(
            transport.operations,
            vec![
                Operation::Reset,
                Operation::Command(COMMAND_SYSTEM_RUN),
                Operation::Command(COMMAND_GET_DEVICE_INFO),
                Operation::Command(COMMAND_VCOM),
                Operation::Write(0),
                Operation::Command(COMMAND_VCOM),
                Operation::Write(1),
                Operation::Write(1_500),
            ]
        );
        assert_eq!(
            info.capabilities().fast_monochrome_constraints,
            UpdateConstraints::new(32, 1, 32, 1)
        );
        assert_eq!(info.fast_monochrome_mode(), 4);
    }

    #[test]
    fn vcom_must_be_explicit_and_plausible() {
        assert_eq!(VcomMillivolts::new(0), None);
        assert_eq!(VcomMillivolts::new(5_001), None);
        assert_eq!(VcomMillivolts::new(1_500).unwrap().get(), 1_500);
    }
}
