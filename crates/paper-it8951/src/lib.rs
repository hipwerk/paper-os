//! Portable, fail-closed IT8951 control protocol.
//!
//! Linux SPI/GPIO framing belongs in a [`Transport`] implementation. This crate
//! owns controller commands, probed identity, VCOM typing, and verified LUT
//! metadata.

#![no_std]

use core::fmt;

use paper_display::{
    Display, DisplayCapabilities, PixelFormat, Rect, Size, UpdateConstraints, UpdateProfile,
    UpdateRequest, Waveform,
};

const COMMAND_SYSTEM_RUN: u16 = 0x0001;
const COMMAND_STANDBY: u16 = 0x0002;
const COMMAND_SLEEP: u16 = 0x0003;
const COMMAND_REGISTER_READ: u16 = 0x0010;
const COMMAND_REGISTER_WRITE: u16 = 0x0011;
const COMMAND_LOAD_IMAGE_AREA: u16 = 0x0021;
const COMMAND_LOAD_IMAGE_END: u16 = 0x0022;
const COMMAND_DISPLAY_BUFFER_AREA: u16 = 0x0037;
const COMMAND_GET_DEVICE_INFO: u16 = 0x0302;
const COMMAND_VCOM: u16 = 0x0039;
const DEVICE_INFO_WORDS: usize = 20;
const REGISTER_PACKED_WRITE: u16 = 0x0004;
const REGISTER_IMAGE_ADDRESS: u16 = 0x0208;
const REGISTER_DISPLAY_STATUS: u16 = 0x1224;
const PIXEL_FORMAT_GRAY4: u16 = 2;
const MODE_INITIALIZE: u16 = 0;
const MODE_GRAYSCALE: u16 = 2;

const IMPLEMENTED_FULL_PROFILES: &[UpdateProfile] = &[
    UpdateProfile::new(
        PixelFormat::Gray4,
        Waveform::Initialize,
        false,
        UpdateConstraints::UNRESTRICTED,
    ),
    UpdateProfile::new(
        PixelFormat::Gray4,
        Waveform::Grayscale,
        false,
        UpdateConstraints::UNRESTRICTED,
    ),
];

/// Bounded polling policy for the IT8951 display engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayWait {
    timeout_ms: u32,
    poll_delay_ms: u32,
}

impl DisplayWait {
    /// Creates a non-zero wall-clock timeout and polling interval.
    pub const fn new(timeout_ms: u32, poll_delay_ms: u32) -> Option<Self> {
        if timeout_ms == 0 || poll_delay_ms == 0 || poll_delay_ms > timeout_ms {
            None
        } else {
            Some(Self {
                timeout_ms,
                poll_delay_ms,
            })
        }
    }
}

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

    /// Returns controller alignment metadata for an allowlisted fast mode.
    ///
    /// This describes a controller fact, not an operation implemented by a
    /// concrete [`Display`] backend.
    pub fn fast_monochrome_constraints(self) -> Option<UpdateConstraints> {
        let family = self.lut_family();
        family.fast_monochrome_mode()?;
        Some(if family.requires_four_byte_alignment() {
            UpdateConstraints::new(32, 1, 32, 1)
                .expect("IT8951 fast-profile alignments are non-zero")
        } else {
            UpdateConstraints::UNRESTRICTED
        })
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

/// Converts four PaperOS Gray4 pixels into the IT8951 packed-write word.
///
/// PaperOS stores pixels from the most-significant nibble of each byte, while
/// the IT8951 little-endian packed format numbers the first pixel from the
/// least-significant nibble of the 16-bit word. For source bytes `01 23`, the
/// controller word is therefore `3210`.
const fn controller_gray4_word(first: u8, second: u8) -> u16 {
    u16::from_le_bytes([first.rotate_left(4), second.rotate_left(4)])
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

    /// Sends host-order data words in one chip-select transaction.
    fn write_words<I>(&mut self, words: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = u16>;

    /// Sends one host-order data word in one transaction.
    fn write_word(&mut self, word: u16) -> Result<(), Self::Error> {
        self.write_words(core::iter::once(word))
    }

    /// Reads controller words into host-order `u16` values.
    ///
    /// IT8951 SPI transfers the most-significant byte first. The transport must
    /// assemble those bytes into the numeric word before returning it. Device
    /// version strings are then decoded from the little-endian in-memory word
    /// representation mandated by the controller's device-info structure.
    fn read_words(&mut self, words: &mut [u16]) -> Result<(), Self::Error>;

    /// Delays portable controller polling without assuming an operating system.
    fn delay_ms(&mut self, milliseconds: u32);

    /// Starts a shared monotonic deadline for one controller operation.
    fn begin_operation(&mut self, timeout_ms: u32);

    /// Returns whether the active operation deadline has expired.
    fn operation_timed_out(&self) -> bool;

    /// Clears the active operation deadline.
    fn end_operation(&mut self);
}

/// Invalid update rejected before a physical refresh command is issued.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateError {
    /// Only a complete native panel update is implemented.
    FullScreenOnly,
    /// The backend does not implement the requested format/waveform pair.
    UnsupportedProfile,
    /// The requested geometry cannot be represented by IT8951 command words.
    GeometryTooLarge,
    /// The source stride is shorter than one encoded row.
    ShortStride,
    /// The source slice does not contain every requested row.
    ShortBuffer,
    /// IT8951 word uploads require an even encoded byte count per row.
    OddRowBytes,
}

impl fmt::Display for UpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::FullScreenOnly => "only full-screen updates are implemented",
            Self::UnsupportedProfile => "unsupported IT8951 format/waveform profile",
            Self::GeometryTooLarge => "update geometry exceeds IT8951 command fields",
            Self::ShortStride => "update stride is shorter than one encoded row",
            Self::ShortBuffer => "update pixel buffer does not contain every source row",
            Self::OddRowBytes => "encoded IT8951 rows must contain an even number of bytes",
        })
    }
}

/// IT8951 control failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error<E> {
    /// Underlying SPI/GPIO transport failure.
    Transport(E),
    /// The controller returned an empty panel size.
    InvalidDeviceInfo,
    /// The controller returned an unusable image-buffer base address.
    InvalidImageBufferAddress(u32),
    /// The controller returned a zero or implausible VCOM magnitude.
    InvalidVcomResponse,
    /// The display engine remained busy for the complete configured budget.
    DisplayTimeout,
    /// Reinitialization found a different controller or panel.
    DeviceChanged {
        /// Identity used to construct the backend.
        expected: DeviceInfo,
        /// Identity observed after reset.
        observed: DeviceInfo,
    },
    /// The update was rejected before upload or refresh.
    InvalidUpdate(UpdateError),
    /// A VCOM write completed at the transport layer but readback did not match.
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
            Self::InvalidImageBufferAddress(address) => write!(
                formatter,
                "IT8951 returned an invalid image-buffer address 0x{address:08x}"
            ),
            Self::InvalidVcomResponse => formatter.write_str("IT8951 returned an invalid VCOM"),
            Self::DisplayTimeout => formatter.write_str("IT8951 display engine timed out"),
            Self::DeviceChanged { .. } => {
                formatter.write_str("IT8951 identity changed while reinitializing after sleep")
            }
            Self::InvalidUpdate(error) => write!(formatter, "invalid IT8951 update: {error}"),
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

impl<E> core::error::Error for Error<E>
where
    E: core::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Transport(error) => Some(error),
            Self::InvalidDeviceInfo
            | Self::InvalidImageBufferAddress(_)
            | Self::InvalidVcomResponse
            | Self::DisplayTimeout
            | Self::DeviceChanged { .. }
            | Self::InvalidUpdate(_)
            | Self::VcomMismatch { .. } => None,
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
        let maximum_buffer_bytes = info
            .panel_size
            .width
            .checked_mul(info.panel_size.height)
            .filter(|bytes| *bytes != 0);
        let address_is_plausible = info.image_buffer_address != 0
            && info.image_buffer_address.is_multiple_of(2)
            && maximum_buffer_bytes.is_some_and(|bytes| {
                info.image_buffer_address
                    .checked_add(bytes.saturating_sub(1))
                    .is_some()
            });
        if !address_is_plausible {
            return Err(Error::InvalidImageBufferAddress(info.image_buffer_address));
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
    /// This changes a panel-health-sensitive setting for the current controller
    /// session. Hardware reset or power loss may restore a controller boot
    /// default, so authorized callers must reapply and verify the exact value
    /// from the panel FPC before every refresh session.
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

    /// Puts a reset or standby controller in system-run state.
    ///
    /// Deep sleep requires reset and reprobe; [`It8951Display::wake`] performs
    /// that complete lifecycle.
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

    fn write_register(&mut self, address: u16, value: u16) -> Result<(), Error<T::Error>> {
        self.transport
            .command(COMMAND_REGISTER_WRITE)
            .map_err(Error::Transport)?;
        self.transport
            .write_word(address)
            .map_err(Error::Transport)?;
        self.transport.write_word(value).map_err(Error::Transport)
    }

    fn read_register(&mut self, address: u16) -> Result<u16, Error<T::Error>> {
        self.transport
            .command(COMMAND_REGISTER_READ)
            .map_err(Error::Transport)?;
        self.transport
            .write_word(address)
            .map_err(Error::Transport)?;
        let mut value = [0];
        self.transport
            .read_words(&mut value)
            .map_err(Error::Transport)?;
        Ok(value[0])
    }

    fn wait_for_display(&mut self, wait: DisplayWait) -> Result<(), Error<T::Error>> {
        self.transport.begin_operation(wait.timeout_ms);
        let result = (|| {
            loop {
                if self.transport.operation_timed_out() {
                    return Err(Error::DisplayTimeout);
                }
                let status = match self.read_register(REGISTER_DISPLAY_STATUS) {
                    Ok(status) => status,
                    Err(_) if self.transport.operation_timed_out() => {
                        return Err(Error::DisplayTimeout);
                    }
                    Err(error) => return Err(error),
                };
                if status == 0 {
                    return Ok(());
                }
                if self.transport.operation_timed_out() {
                    return Err(Error::DisplayTimeout);
                }
                self.transport.delay_ms(wait.poll_delay_ms);
            }
        })();
        self.transport.end_operation();
        result
    }

    /// Returns the owned transport.
    pub fn into_transport(self) -> T {
        self.transport
    }
}

/// Conservative full-screen IT8951 display backend.
///
/// Construction does not touch hardware. Callers first probe and verify the
/// returned identity and VCOM, then pass the still-awake controller here.
pub struct It8951Display<T> {
    controller: Controller<T>,
    device_info: DeviceInfo,
    expected_vcom: VcomMillivolts,
    capabilities: DisplayCapabilities,
    wait: DisplayWait,
}

impl<T: Transport> It8951Display<T> {
    /// Builds a display from an already-probed controller.
    pub const fn new(controller: Controller<T>, report: ProbeReport, wait: DisplayWait) -> Self {
        Self {
            controller,
            device_info: report.device_info,
            expected_vcom: report.current_vcom,
            capabilities: DisplayCapabilities {
                native_size: report.device_info.panel_size,
                update_profiles: IMPLEMENTED_FULL_PROFILES,
            },
            wait,
        }
    }

    /// Returns the owned controller for explicit cleanup or further diagnostics.
    pub fn into_controller(self) -> Controller<T> {
        self.controller
    }

    fn validate(&self, request: &UpdateRequest<'_>) -> Result<usize, UpdateError> {
        if request.region != Rect::from_size(self.device_info.panel_size) {
            return Err(UpdateError::FullScreenOnly);
        }
        if self
            .capabilities
            .profile(request.pixel_format, request.waveform)
            .is_none()
        {
            return Err(UpdateError::UnsupportedProfile);
        }
        if request.region.size.width > u32::from(u16::MAX)
            || request.region.size.height > u32::from(u16::MAX)
        {
            return Err(UpdateError::GeometryTooLarge);
        }
        let row_bytes = request
            .pixel_format
            .row_bytes(request.region.size.width)
            .ok_or(UpdateError::GeometryTooLarge)?;
        if row_bytes % 2 != 0 {
            return Err(UpdateError::OddRowBytes);
        }
        if request.stride_bytes < row_bytes {
            return Err(UpdateError::ShortStride);
        }
        let last_row = request
            .region
            .size
            .height
            .checked_sub(1)
            .and_then(|rows| usize::try_from(rows).ok())
            .and_then(|rows| rows.checked_mul(request.stride_bytes))
            .and_then(|offset| offset.checked_add(row_bytes))
            .ok_or(UpdateError::GeometryTooLarge)?;
        if request.pixels.len() < last_row {
            return Err(UpdateError::ShortBuffer);
        }
        Ok(row_bytes)
    }

    fn update_inner(&mut self, request: UpdateRequest<'_>) -> Result<(), Error<T::Error>> {
        let row_bytes = self.validate(&request).map_err(Error::InvalidUpdate)?;
        let width = u16::try_from(request.region.size.width)
            .map_err(|_| Error::InvalidUpdate(UpdateError::GeometryTooLarge))?;
        let height = u16::try_from(request.region.size.height)
            .map_err(|_| Error::InvalidUpdate(UpdateError::GeometryTooLarge))?;
        let address = self.device_info.image_buffer_address.to_le_bytes();
        self.controller.wait_for_display(self.wait)?;
        self.controller.write_register(
            REGISTER_IMAGE_ADDRESS + 2,
            u16::from_le_bytes([address[2], address[3]]),
        )?;
        self.controller.write_register(
            REGISTER_IMAGE_ADDRESS,
            u16::from_le_bytes([address[0], address[1]]),
        )?;

        let format = match request.pixel_format {
            PixelFormat::Gray4 => PIXEL_FORMAT_GRAY4,
            PixelFormat::Gray8 | PixelFormat::Gray2 | PixelFormat::Monochrome1 => {
                return Err(Error::InvalidUpdate(UpdateError::UnsupportedProfile));
            }
        };
        if request.pixel_format == PixelFormat::Gray4 {
            self.controller.write_register(REGISTER_PACKED_WRITE, 1)?;
        }
        self.controller
            .transport
            .command(COMMAND_LOAD_IMAGE_AREA)
            .map_err(Error::Transport)?;
        for argument in [format << 4, 0, 0, width, height] {
            self.controller
                .transport
                .write_word(argument)
                .map_err(Error::Transport)?;
        }

        let words = (0..request.region.size.height as usize).flat_map(|row| {
            let start = row * request.stride_bytes;
            request.pixels[start..start + row_bytes]
                .chunks_exact(2)
                .map(|pair| controller_gray4_word(pair[0], pair[1]))
        });
        self.controller
            .transport
            .write_words(words)
            .map_err(Error::Transport)?;
        self.controller
            .transport
            .command(COMMAND_LOAD_IMAGE_END)
            .map_err(Error::Transport)?;

        let mode = match request.waveform {
            Waveform::Initialize => MODE_INITIALIZE,
            Waveform::Grayscale => MODE_GRAYSCALE,
            Waveform::FastMonochrome => {
                return Err(Error::InvalidUpdate(UpdateError::UnsupportedProfile));
            }
        };
        self.controller
            .transport
            .command(COMMAND_DISPLAY_BUFFER_AREA)
            .map_err(Error::Transport)?;
        let address_low = u16::from_le_bytes([address[0], address[1]]);
        let address_high = u16::from_le_bytes([address[2], address[3]]);
        for argument in [0, 0, width, height, mode, address_low, address_high] {
            self.controller
                .transport
                .write_word(argument)
                .map_err(Error::Transport)?;
        }
        self.controller.wait_for_display(self.wait)
    }
}

impl<T> Display for It8951Display<T>
where
    T: Transport,
    T::Error: fmt::Debug,
{
    type Error = Error<T::Error>;

    fn capabilities(&self) -> &DisplayCapabilities {
        &self.capabilities
    }

    fn update(&mut self, request: UpdateRequest<'_>) -> Result<(), Self::Error> {
        self.update_inner(request)
    }

    fn sleep(&mut self) -> Result<(), Self::Error> {
        self.controller.sleep()
    }

    fn wake(&mut self) -> Result<(), Self::Error> {
        let report = self.controller.probe()?;
        if report.device_info != self.device_info {
            return Err(Error::DeviceChanged {
                expected: self.device_info,
                observed: report.device_info,
            });
        }
        if report.current_vcom != self.expected_vcom {
            self.controller.set_vcom(self.expected_vcom)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec;
    use std::vec::Vec;

    use paper_display::{
        Display, PixelFormat, Rect, Size, UpdateConstraints, UpdateRequest, Waveform,
    };

    use super::{
        COMMAND_DISPLAY_BUFFER_AREA, COMMAND_GET_DEVICE_INFO, COMMAND_LOAD_IMAGE_AREA,
        COMMAND_LOAD_IMAGE_END, COMMAND_SYSTEM_RUN, COMMAND_VCOM, Controller, DeviceInfo,
        DisplayWait, Error, It8951Display, LutFamily, ProbeReport, Transport, UpdateError,
        VcomMillivolts, controller_gray4_word,
    };

    #[derive(Default)]
    struct FakeTransport {
        operations: Vec<Operation>,
        reads: Vec<u16>,
        elapsed_ms: u32,
        deadline_ms: Option<u32>,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Operation {
        Reset,
        Command(u16),
        Write(u16),
        WriteMany(Vec<u16>),
        Delay(u32),
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

        fn write_words<I>(&mut self, words: I) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = u16>,
        {
            self.operations
                .push(Operation::WriteMany(words.into_iter().collect()));
            Ok(())
        }

        fn read_words(&mut self, words: &mut [u16]) -> Result<(), Self::Error> {
            words.copy_from_slice(&self.reads[..words.len()]);
            self.reads.drain(..words.len());
            Ok(())
        }

        fn delay_ms(&mut self, milliseconds: u32) {
            self.operations.push(Operation::Delay(milliseconds));
            let remaining = self.deadline_ms.map_or(milliseconds, |deadline| {
                deadline.saturating_sub(self.elapsed_ms)
            });
            self.elapsed_ms = self.elapsed_ms.saturating_add(milliseconds.min(remaining));
        }

        fn begin_operation(&mut self, timeout_ms: u32) {
            self.deadline_ms = Some(self.elapsed_ms.saturating_add(timeout_ms));
        }

        fn operation_timed_out(&self) -> bool {
            self.deadline_ms
                .is_some_and(|deadline| self.elapsed_ms >= deadline)
        }

        fn end_operation(&mut self) {
            self.deadline_ms = None;
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

    fn probe_report(info: DeviceInfo) -> ProbeReport {
        ProbeReport {
            device_info: info,
            current_vcom: VcomMillivolts::new(1_500).unwrap(),
        }
    }

    fn words_for_device_info(info: DeviceInfo) -> Vec<u16> {
        let address = info.image_buffer_address.to_le_bytes();
        let mut words = vec![
            u16::try_from(info.panel_size.width).unwrap(),
            u16::try_from(info.panel_size.height).unwrap(),
            u16::from_le_bytes([address[0], address[1]]),
            u16::from_le_bytes([address[2], address[3]]),
        ];
        words.extend(
            info.firmware_version
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]])),
        );
        words.extend(
            info.lut_version
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]])),
        );
        words
    }

    #[test]
    fn probe_reads_vcom_without_writing_it() {
        let mut reads = vec![1448, 1072, 0x1234, 0x5678, 0x3646, 0, 0, 0, 0, 0, 0, 0];
        reads.extend(words_for_lut(b"M641"));
        reads.push(1_400);
        let transport = FakeTransport {
            reads,
            ..FakeTransport::default()
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
    fn device_info_rejects_unusable_image_buffer_addresses() {
        for address in [0, 1, u32::MAX - 1] {
            let info = DeviceInfo {
                image_buffer_address: address,
                ..device_info(b"M641")
            };
            let transport = FakeTransport {
                reads: words_for_device_info(info),
                ..FakeTransport::default()
            };
            let mut controller = Controller::new(transport);

            assert_eq!(
                controller.device_info(),
                Err(Error::InvalidImageBufferAddress(address))
            );
        }
    }

    #[test]
    fn setting_vcom_is_a_separate_explicit_operation() {
        let transport = FakeTransport {
            reads: vec![1_500],
            ..FakeTransport::default()
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
            reads: vec![1_400],
            ..FakeTransport::default()
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
                info.fast_monochrome_constraints().unwrap(),
                UpdateConstraints::new(32, 1, 32, 1).unwrap()
            );
        }
    }

    #[test]
    fn unknown_lut_does_not_advertise_or_guess_fast_mode() {
        let info = device_info(b"FUTURE_UNKNOWN");
        assert_eq!(info.lut_family(), LutFamily::Unknown);
        assert_eq!(info.fast_monochrome_mode(), None);
        assert_eq!(info.fast_monochrome_constraints(), None);
    }

    #[test]
    fn vcom_must_be_explicit_and_plausible() {
        assert_eq!(VcomMillivolts::new(0), None);
        assert_eq!(VcomMillivolts::new(5_001), None);
        assert_eq!(VcomMillivolts::new(1_500).unwrap().get(), 1_500);
    }

    #[test]
    fn packed_gray4_full_update_uses_one_bulk_transaction() {
        let info = device_info(b"M641");
        let pixels = [0x01, 0x23, 0x45, 0x67];

        let small = DeviceInfo {
            panel_size: Size::new(4, 2),
            image_buffer_address: 0x1234_5678,
            ..info
        };
        let transport = FakeTransport {
            reads: vec![0, 0],
            ..FakeTransport::default()
        };
        let mut display = It8951Display::new(
            Controller::new(transport),
            probe_report(small),
            DisplayWait::new(3, 1).unwrap(),
        );
        display
            .update(UpdateRequest {
                region: Rect::from_size(small.panel_size),
                pixel_format: PixelFormat::Gray4,
                stride_bytes: 2,
                pixels: &pixels,
                waveform: Waveform::Initialize,
            })
            .unwrap();
        let operations = display.into_controller().into_transport().operations;

        let load_command = operations
            .iter()
            .position(|operation| *operation == Operation::Command(COMMAND_LOAD_IMAGE_AREA))
            .unwrap();
        assert_eq!(
            &operations[load_command + 1..load_command + 8],
            &[
                Operation::Write(0x20),
                Operation::Write(0),
                Operation::Write(0),
                Operation::Write(4),
                Operation::Write(2),
                Operation::WriteMany(vec![0x3210, 0x7654]),
                Operation::Command(COMMAND_LOAD_IMAGE_END),
            ]
        );
        let display_command = operations
            .iter()
            .position(|operation| *operation == Operation::Command(COMMAND_DISPLAY_BUFFER_AREA))
            .unwrap();
        assert_eq!(
            &operations[display_command + 1..display_command + 8],
            &[
                Operation::Write(0),
                Operation::Write(0),
                Operation::Write(4),
                Operation::Write(2),
                Operation::Write(0),
                Operation::Write(0x5678),
                Operation::Write(0x1234),
            ]
        );
    }

    #[test]
    fn gray4_adapter_matches_controller_pixel_numbering() {
        // PaperOS bytes 01 23 encode left-to-right pixels 0, 1, 2, 3.
        // IT8951 packed-write Figure 7-17 places the first pixel in the low nibble.
        assert_eq!(controller_gray4_word(0x01, 0x23), 0x3210);
    }

    #[test]
    fn gray8_is_not_advertised_or_executed_by_the_physical_backend() {
        let info = DeviceInfo {
            panel_size: Size::new(4, 1),
            image_buffer_address: 0x1234_5678,
            ..device_info(b"M641")
        };
        let transport = FakeTransport::default();
        let mut display = It8951Display::new(
            Controller::new(transport),
            probe_report(info),
            DisplayWait::new(3, 1).unwrap(),
        );

        assert!(
            display
                .capabilities()
                .profile(PixelFormat::Gray8, Waveform::Grayscale)
                .is_none()
        );
        assert_eq!(
            display.update(UpdateRequest {
                region: Rect::from_size(info.panel_size),
                pixel_format: PixelFormat::Gray8,
                stride_bytes: 4,
                pixels: &[0, 64, 128, 255],
                waveform: Waveform::Grayscale,
            }),
            Err(Error::InvalidUpdate(UpdateError::UnsupportedProfile))
        );
        assert!(
            display
                .into_controller()
                .into_transport()
                .operations
                .is_empty()
        );
    }

    #[test]
    fn display_busy_polling_is_bounded() {
        let info = DeviceInfo {
            panel_size: Size::new(4, 1),
            ..device_info(b"M641")
        };
        let transport = FakeTransport {
            reads: vec![1, 1, 1],
            ..FakeTransport::default()
        };
        let mut display = It8951Display::new(
            Controller::new(transport),
            probe_report(info),
            DisplayWait::new(14, 7).unwrap(),
        );

        assert_eq!(
            display.update(UpdateRequest {
                region: Rect::from_size(info.panel_size),
                pixel_format: PixelFormat::Gray4,
                stride_bytes: 2,
                pixels: &[255; 2],
                waveform: Waveform::Grayscale,
            }),
            Err(Error::DisplayTimeout)
        );
        assert_eq!(
            display.into_controller().into_transport().operations,
            vec![
                Operation::Command(super::COMMAND_REGISTER_READ),
                Operation::Write(super::REGISTER_DISPLAY_STATUS),
                Operation::Delay(7),
                Operation::Command(super::COMMAND_REGISTER_READ),
                Operation::Write(super::REGISTER_DISPLAY_STATUS),
                Operation::Delay(7),
            ]
        );
    }

    #[test]
    fn wake_resets_reprobes_and_revalidates_identity_and_vcom() {
        let info = device_info(b"M641");
        let mut reads = words_for_device_info(info);
        reads.push(1_500);
        let transport = FakeTransport {
            reads,
            ..FakeTransport::default()
        };
        let mut display = It8951Display::new(
            Controller::new(transport),
            probe_report(info),
            DisplayWait::new(3, 1).unwrap(),
        );

        display.sleep().unwrap();
        display.wake().unwrap();

        let operations = display.into_controller().into_transport().operations;
        assert!(operations.contains(&Operation::Reset));
        assert!(operations.contains(&Operation::Command(COMMAND_GET_DEVICE_INFO)));
        assert!(operations.contains(&Operation::Command(COMMAND_VCOM)));
    }

    #[test]
    fn wake_reapplies_expected_vcom_after_reprobe() {
        let info = device_info(b"M641");
        let mut reads = words_for_device_info(info);
        reads.push(1_400);
        reads.push(1_500);
        let transport = FakeTransport {
            reads,
            ..FakeTransport::default()
        };
        let mut display = It8951Display::new(
            Controller::new(transport),
            probe_report(info),
            DisplayWait::new(3, 1).unwrap(),
        );

        display.wake().unwrap();
        let operations = display.into_controller().into_transport().operations;
        assert!(operations.windows(4).any(|window| {
            window
                == [
                    Operation::Command(COMMAND_VCOM),
                    Operation::Write(1),
                    Operation::Write(1_500),
                    Operation::Command(COMMAND_VCOM),
                ]
        }));
    }

    #[test]
    fn wake_rejects_changed_identity_before_writing_vcom() {
        let expected = device_info(b"M641");
        let observed = DeviceInfo {
            image_buffer_address: 0x1234_567a,
            ..expected
        };
        let mut reads = words_for_device_info(observed);
        reads.push(1_400);
        let transport = FakeTransport {
            reads,
            ..FakeTransport::default()
        };
        let mut display = It8951Display::new(
            Controller::new(transport),
            probe_report(expected),
            DisplayWait::new(3, 1).unwrap(),
        );

        assert!(matches!(display.wake(), Err(Error::DeviceChanged { .. })));
        let operations = display.into_controller().into_transport().operations;
        assert!(
            !operations.windows(2).any(|window| {
                window == [Operation::Command(COMMAND_VCOM), Operation::Write(1)]
            })
        );
    }

    #[test]
    fn odd_packed_row_is_rejected_before_transport() {
        let info = DeviceInfo {
            panel_size: Size::new(2, 1),
            ..device_info(b"M641")
        };
        let transport = FakeTransport::default();
        let mut display = It8951Display::new(
            Controller::new(transport),
            probe_report(info),
            DisplayWait::new(1, 1).unwrap(),
        );

        assert_eq!(
            display.update(UpdateRequest {
                region: Rect::from_size(info.panel_size),
                pixel_format: PixelFormat::Gray4,
                stride_bytes: 1,
                pixels: &[0xff],
                waveform: Waveform::Initialize,
            }),
            Err(Error::InvalidUpdate(UpdateError::OddRowBytes))
        );
        assert!(
            display
                .into_controller()
                .into_transport()
                .operations
                .is_empty()
        );
    }
}
