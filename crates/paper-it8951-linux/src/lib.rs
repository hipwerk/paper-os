//! Linux host support for the portable IT8951 protocol.
//!
//! The transaction engine is platform-neutral for deterministic tests. The
//! concrete Linux backend uses spidev with hardware chip select disabled and
//! GPIO character-device ABI v2 for manual CS, reset, and HRDY.

use std::error::Error as StdError;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use paper_it8951::{DisplayWait, Transport, VcomMillivolts};
use serde::Deserialize;

const COMMAND_PREAMBLE: [u8; 2] = [0x60, 0x00];
const WRITE_PREAMBLE: [u8; 2] = [0x00, 0x00];
const READ_PREAMBLE: [u8; 2] = [0x10, 0x00];
const WORD_BUFFER_LEN: usize = 256;

/// Highest SPI rate audited in Waveshare's Raspberry Pi IT8951 implementation.
///
/// Initial bring-up should remain at 1 MHz. Raising the local profile is a
/// measured lab decision and cannot exceed this vendor-proven ceiling.
pub const MAX_AUDITED_SPI_HZ: u32 = 12_500_000;

/// SPI operations needed while chip select is controlled separately.
pub trait SpiBus {
    /// Writes every byte or returns an I/O error.
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()>;

    /// Clocks bytes from the device until the destination is full.
    fn read_exact(&mut self, bytes: &mut [u8]) -> io::Result<()>;
}

/// GPIO operations needed by the IT8951 transaction engine.
pub trait Pins {
    /// Drives active-low chip select.
    fn set_chip_select(&mut self, high: bool) -> io::Result<()>;

    /// Drives active-low reset.
    fn set_reset(&mut self, high: bool) -> io::Result<()>;

    /// Returns true when HRDY is high and the controller is ready.
    fn is_ready(&mut self) -> io::Result<bool>;
}

/// Timing applied to ready polling and hardware reset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timing {
    /// Maximum wait for one HRDY synchronization.
    pub ready_timeout: Duration,
    /// Delay between HRDY samples.
    pub ready_poll_interval: Duration,
    /// Initial reset-high settling time.
    pub reset_high: Duration,
    /// Active-low reset pulse.
    pub reset_low: Duration,
    /// Recovery time after reset is released.
    pub reset_recovery: Duration,
}

impl Timing {
    /// Returns whether every duration is non-zero.
    pub fn is_valid(self) -> bool {
        !self.ready_timeout.is_zero()
            && !self.ready_poll_interval.is_zero()
            && !self.reset_high.is_zero()
            && !self.reset_low.is_zero()
            && !self.reset_recovery.is_zero()
    }
}

/// Linux transport failure.
#[derive(Debug)]
pub enum TransportError {
    /// SPI or GPIO operation failed.
    Io(io::Error),
    /// HRDY remained low for the configured transaction budget.
    ReadyTimeout(Duration),
    /// A shared controller-operation deadline expired.
    OperationTimeout,
    /// A zero timing duration would make timeout behavior ambiguous.
    InvalidTiming,
    /// A transaction failed and manual chip select could not be released.
    TransactionAndRelease {
        /// Original transaction failure.
        transaction: Box<Self>,
        /// Failure while driving chip select high.
        release: io::Error,
    },
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "Linux IT8951 I/O failed: {error}"),
            Self::ReadyTimeout(timeout) => {
                write!(formatter, "IT8951 HRDY timed out after {timeout:?}")
            }
            Self::OperationTimeout => formatter.write_str("IT8951 operation deadline expired"),
            Self::InvalidTiming => formatter.write_str("IT8951 timing values must be non-zero"),
            Self::TransactionAndRelease {
                transaction,
                release,
            } => write!(
                formatter,
                "IT8951 transaction failed: {transaction}; manual CS release also failed: {release}"
            ),
        }
    }
}

impl StdError for TransportError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::TransactionAndRelease { transaction, .. } => Some(transaction),
            Self::ReadyTimeout(_) | Self::OperationTimeout | Self::InvalidTiming => None,
        }
    }
}

impl From<io::Error> for TransportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Manual-CS transaction implementation shared by Linux and host-side tests.
pub struct ManualCsTransport<S, P> {
    spi: S,
    pins: P,
    timing: Timing,
    operation_deadline: Option<Instant>,
}

impl<S, P> ManualCsTransport<S, P>
where
    S: SpiBus,
    P: Pins,
{
    /// Creates a transport without touching its SPI or GPIO resources.
    pub fn new(spi: S, pins: P, timing: Timing) -> Result<Self, TransportError> {
        if !timing.is_valid() {
            return Err(TransportError::InvalidTiming);
        }
        Ok(Self {
            spi,
            pins,
            timing,
            operation_deadline: None,
        })
    }

    /// Returns the owned low-level resources.
    pub fn into_parts(self) -> (S, P) {
        (self.spi, self.pins)
    }

    fn wait_ready(&mut self) -> Result<(), TransportError> {
        let started = Instant::now();
        loop {
            if self.operation_timed_out() {
                return Err(TransportError::OperationTimeout);
            }
            if self.pins.is_ready()? {
                return Ok(());
            }
            if started.elapsed() >= self.timing.ready_timeout {
                return Err(TransportError::ReadyTimeout(self.timing.ready_timeout));
            }
            let mut delay = self.timing.ready_poll_interval;
            if let Some(deadline) = self.operation_deadline {
                delay = delay.min(deadline.saturating_duration_since(Instant::now()));
            }
            if !delay.is_zero() {
                thread::sleep(delay);
            }
        }
    }

    fn selected<F>(&mut self, operation: F) -> Result<(), TransportError>
    where
        F: FnOnce(&mut Self) -> Result<(), TransportError>,
    {
        self.pins.set_chip_select(false)?;
        let result = operation(self);
        let release = self.pins.set_chip_select(true);
        match (result, release) {
            (Err(transaction), Err(release)) => Err(TransportError::TransactionAndRelease {
                transaction: Box::new(transaction),
                release,
            }),
            (Err(error), Ok(())) => Err(error),
            (Ok(()), Err(error)) => Err(error.into()),
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    fn write_transaction(
        &mut self,
        preamble: [u8; 2],
        payload: [u8; 2],
    ) -> Result<(), TransportError> {
        self.wait_ready()?;
        self.selected(|transport| {
            transport.spi.write_all(&preamble)?;
            transport.wait_ready()?;
            transport.spi.write_all(&payload)?;
            Ok(())
        })
    }
}

impl<S, P> Transport for ManualCsTransport<S, P>
where
    S: SpiBus,
    P: Pins,
{
    type Error = TransportError;

    fn reset(&mut self) -> Result<(), Self::Error> {
        self.pins.set_chip_select(true)?;
        self.pins.set_reset(true)?;
        thread::sleep(self.timing.reset_high);
        self.pins.set_reset(false)?;
        thread::sleep(self.timing.reset_low);
        self.pins.set_reset(true)?;
        thread::sleep(self.timing.reset_recovery);
        self.wait_ready()
    }

    fn command(&mut self, command: u16) -> Result<(), Self::Error> {
        self.write_transaction(COMMAND_PREAMBLE, command.to_be_bytes())
    }

    fn write_words<I>(&mut self, words: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = u16>,
    {
        self.wait_ready()?;
        self.selected(|transport| {
            transport.spi.write_all(&WRITE_PREAMBLE)?;
            transport.wait_ready()?;
            let mut buffer = [0_u8; WORD_BUFFER_LEN * 2];
            let mut used = 0;
            for word in words {
                let bytes = word.to_be_bytes();
                buffer[used] = bytes[0];
                buffer[used + 1] = bytes[1];
                used += 2;
                if used == buffer.len() {
                    transport.spi.write_all(&buffer)?;
                    used = 0;
                }
            }
            if used != 0 {
                transport.spi.write_all(&buffer[..used])?;
            }
            Ok(())
        })
    }

    fn read_words(&mut self, words: &mut [u16]) -> Result<(), Self::Error> {
        self.wait_ready()?;
        self.selected(|transport| {
            transport.spi.write_all(&READ_PREAMBLE)?;
            transport.wait_ready()?;
            let mut dummy = [0_u8; 2];
            transport.spi.read_exact(&mut dummy)?;
            transport.wait_ready()?;
            for word in words {
                let mut bytes = [0_u8; 2];
                transport.spi.read_exact(&mut bytes)?;
                *word = u16::from_be_bytes(bytes);
            }
            Ok(())
        })
    }

    fn delay_ms(&mut self, milliseconds: u32) {
        let requested = Duration::from_millis(u64::from(milliseconds));
        let delay = self.operation_deadline.map_or(requested, |deadline| {
            requested.min(deadline.saturating_duration_since(Instant::now()))
        });
        if !delay.is_zero() {
            thread::sleep(delay);
        }
    }

    fn begin_operation(&mut self, timeout_ms: u32) {
        self.operation_deadline =
            Some(Instant::now() + Duration::from_millis(u64::from(timeout_ms)));
    }

    fn operation_timed_out(&self) -> bool {
        self.operation_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
    }

    fn end_operation(&mut self) {
        self.operation_deadline = None;
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Profiles {
    panel: Vec<RawPanelProfile>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPanelProfile {
    name: String,
    controller: String,
    width: u32,
    height: u32,
    vcom_mv: u16,
    expected_firmware: Option<String>,
    expected_lut: Option<String>,
    spi_device: PathBuf,
    gpio_chip: PathBuf,
    cs_line: u32,
    reset_line: u32,
    ready_line: u32,
    max_spi_hz: u32,
    ready_timeout_ms: u64,
    ready_poll_us: u64,
    reset_high_ms: u64,
    reset_low_ms: u64,
    reset_recovery_ms: u64,
    display_timeout_ms: u32,
    display_poll_ms: u32,
}

/// Validated configuration for one explicitly named physical panel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PanelProfile {
    /// Stable local fixture name.
    pub name: String,
    /// Expected native panel dimensions.
    pub panel_size: paper_display::Size,
    /// Exact VCOM magnitude recorded from this panel's FPC.
    pub vcom: VcomMillivolts,
    /// Exact firmware string pinned after the first successful probe.
    pub expected_firmware: Option<String>,
    /// Exact LUT string pinned after the first successful probe.
    pub expected_lut: Option<String>,
    /// Linux spidev node configured with hardware CS disabled.
    pub spi_device: PathBuf,
    /// Linux GPIO character-device node.
    pub gpio_chip: PathBuf,
    /// Manual active-low chip-select line offset.
    pub cs_line: u32,
    /// Active-low reset line offset.
    pub reset_line: u32,
    /// Active-high HRDY line offset.
    pub ready_line: u32,
    /// Maximum SPI clock, capped at [`MAX_AUDITED_SPI_HZ`].
    pub max_spi_hz: u32,
    /// Transaction/reset timing.
    pub timing: Timing,
    /// Bounded display-engine polling policy.
    pub display_wait: DisplayWait,
}

/// Panel-profile loading or validation failure.
#[derive(Debug)]
pub enum ProfileError {
    /// The profile file could not be read.
    Read(io::Error),
    /// TOML syntax or schema validation failed.
    Parse(toml::de::Error),
    /// No profile has the requested exact name.
    NotFound(String),
    /// More than one profile has the requested name.
    DuplicateName(String),
    /// A profile field is unsafe, ambiguous, or unsupported.
    Invalid {
        /// Profile name, when available.
        profile: String,
        /// Human-readable invariant.
        reason: &'static str,
    },
}

impl fmt::Display for ProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(formatter, "could not read panel profiles: {error}"),
            Self::Parse(error) => write!(formatter, "invalid panel profile TOML: {error}"),
            Self::NotFound(name) => write!(formatter, "panel profile {name:?} was not found"),
            Self::DuplicateName(name) => {
                write!(formatter, "panel profile name {name:?} is duplicated")
            }
            Self::Invalid { profile, reason } => {
                write!(formatter, "invalid panel profile {profile:?}: {reason}")
            }
        }
    }
}

impl StdError for ProfileError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            Self::Parse(error) => Some(error),
            Self::NotFound(_) | Self::DuplicateName(_) | Self::Invalid { .. } => None,
        }
    }
}

/// Loads and validates exactly one named local panel profile.
pub fn load_panel_profile(path: &Path, name: &str) -> Result<PanelProfile, ProfileError> {
    let source = fs::read_to_string(path).map_err(ProfileError::Read)?;
    load_panel_profile_from_str(&source, name)
}

fn load_panel_profile_from_str(source: &str, name: &str) -> Result<PanelProfile, ProfileError> {
    let profiles: Profiles = toml::from_str(source).map_err(ProfileError::Parse)?;
    let mut matching = profiles
        .panel
        .into_iter()
        .filter(|panel| panel.name == name);
    let raw = matching
        .next()
        .ok_or_else(|| ProfileError::NotFound(name.to_owned()))?;
    if matching.next().is_some() {
        return Err(ProfileError::DuplicateName(name.to_owned()));
    }
    validate_profile(raw)
}

fn validate_profile(raw: RawPanelProfile) -> Result<PanelProfile, ProfileError> {
    let invalid = |reason| ProfileError::Invalid {
        profile: raw.name.clone(),
        reason,
    };
    if raw.controller != "it8951" {
        return Err(invalid("controller must be \"it8951\""));
    }
    if raw.width == 0
        || raw.height == 0
        || raw.width > u32::from(u16::MAX)
        || raw.height > u32::from(u16::MAX)
    {
        return Err(invalid("dimensions must be non-zero IT8951 u16 values"));
    }
    let vcom =
        VcomMillivolts::new(raw.vcom_mv).ok_or_else(|| invalid("vcom_mv must be 1..=5000"))?;
    if !(1..=MAX_AUDITED_SPI_HZ).contains(&raw.max_spi_hz) {
        return Err(invalid(
            "max_spi_hz must be 1..=12500000 (start bring-up at 1000000)",
        ));
    }
    let valid_version = |version: &str| {
        !version.trim().is_empty()
            && version.len() <= 16
            && version.is_ascii()
            && !version.as_bytes().contains(&0)
    };
    if raw
        .expected_firmware
        .as_deref()
        .is_some_and(|version| !valid_version(version))
    {
        return Err(invalid(
            "expected_firmware must be 1..=16 non-blank ASCII bytes",
        ));
    }
    if raw
        .expected_lut
        .as_deref()
        .is_some_and(|version| !valid_version(version))
    {
        return Err(invalid("expected_lut must be 1..=16 non-blank ASCII bytes"));
    }
    if raw.cs_line == raw.reset_line
        || raw.cs_line == raw.ready_line
        || raw.reset_line == raw.ready_line
    {
        return Err(invalid(
            "cs_line, reset_line, and ready_line must be distinct",
        ));
    }
    let timing = Timing {
        ready_timeout: Duration::from_millis(raw.ready_timeout_ms),
        ready_poll_interval: Duration::from_micros(raw.ready_poll_us),
        reset_high: Duration::from_millis(raw.reset_high_ms),
        reset_low: Duration::from_millis(raw.reset_low_ms),
        reset_recovery: Duration::from_millis(raw.reset_recovery_ms),
    };
    if !timing.is_valid() {
        return Err(invalid(
            "all reset and ready timing fields must be non-zero",
        ));
    }
    if raw.ready_timeout_ms > 10_000
        || raw.ready_poll_us > 10_000
        || raw.reset_high_ms > 5_000
        || raw.reset_low_ms > 1_000
        || raw.reset_recovery_ms > 5_000
    {
        return Err(invalid(
            "transaction/reset timing exceeds the safe setup bounds",
        ));
    }
    let display_wait = DisplayWait::new(raw.display_timeout_ms, raw.display_poll_ms)
        .ok_or_else(|| invalid("display timeout must include at least one polling interval"))?;
    if raw.display_timeout_ms > 120_000 {
        return Err(invalid("display timeout exceeds 120 seconds"));
    }

    Ok(PanelProfile {
        name: raw.name,
        panel_size: paper_display::Size::new(raw.width, raw.height),
        vcom,
        expected_firmware: raw.expected_firmware,
        expected_lut: raw.expected_lut,
        spi_device: raw.spi_device,
        gpio_chip: raw.gpio_chip,
        cs_line: raw.cs_line,
        reset_line: raw.reset_line,
        ready_line: raw.ready_line,
        max_spi_hz: raw.max_spi_hz,
        timing,
        display_wait,
    })
}

/// Concrete Linux spidev and GPIO-v2 resources.
#[cfg(target_os = "linux")]
pub mod system {
    use std::io::{Read, Write};

    use gpiocdev::Request;
    use gpiocdev::line::Value;
    use spidev::{SpiModeFlags, Spidev, SpidevOptions};

    use super::{ManualCsTransport, PanelProfile, Pins, SpiBus, TransportError, io};

    /// Linux spidev bus configured for mode 0 and manual chip select.
    pub struct SystemSpi {
        device: Spidev,
    }

    impl SystemSpi {
        fn open(profile: &PanelProfile) -> io::Result<Self> {
            let mut device = Spidev::open(&profile.spi_device)?;
            let options = SpidevOptions::new()
                .bits_per_word(8)
                .max_speed_hz(profile.max_spi_hz)
                .mode(SpiModeFlags::SPI_MODE_0 | SpiModeFlags::SPI_NO_CS)
                .build();
            device.configure(&options)?;
            Ok(Self { device })
        }
    }

    impl SpiBus for SystemSpi {
        fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
            self.device.write_all(bytes)
        }

        fn read_exact(&mut self, bytes: &mut [u8]) -> io::Result<()> {
            self.device.read_exact(bytes)
        }
    }

    /// Three GPIO-v2 line requests retained for the transport lifetime.
    pub struct SystemPins {
        chip_select: Request,
        reset: Request,
        ready: Request,
    }

    impl SystemPins {
        fn open(profile: &PanelProfile) -> io::Result<Self> {
            let chip = &profile.gpio_chip;
            let chip_select = Request::builder()
                .on_chip(chip)
                .with_consumer("paperos-it8951-cs")
                .with_line(profile.cs_line)
                .as_output(Value::Active)
                .request()
                .map_err(io::Error::other)?;
            let reset = Request::builder()
                .on_chip(chip)
                .with_consumer("paperos-it8951-reset")
                .with_line(profile.reset_line)
                .as_output(Value::Active)
                .request()
                .map_err(io::Error::other)?;
            let ready = Request::builder()
                .on_chip(chip)
                .with_consumer("paperos-it8951-ready")
                .with_line(profile.ready_line)
                .as_input()
                .request()
                .map_err(io::Error::other)?;
            Ok(Self {
                chip_select,
                reset,
                ready,
            })
        }
    }

    impl Pins for SystemPins {
        fn set_chip_select(&mut self, high: bool) -> io::Result<()> {
            self.chip_select
                .set_lone_value(if high { Value::Active } else { Value::Inactive })
                .map_err(io::Error::other)
        }

        fn set_reset(&mut self, high: bool) -> io::Result<()> {
            self.reset
                .set_lone_value(if high { Value::Active } else { Value::Inactive })
                .map_err(io::Error::other)
        }

        fn is_ready(&mut self) -> io::Result<bool> {
            self.ready
                .lone_value()
                .map(|value| value == Value::Active)
                .map_err(io::Error::other)
        }
    }

    /// Concrete Linux IT8951 transport.
    pub type SystemTransport = ManualCsTransport<SystemSpi, SystemPins>;

    /// Opens the configured spidev and GPIO resources without resetting hardware.
    pub fn open(profile: &PanelProfile) -> Result<SystemTransport, TransportError> {
        let spi = SystemSpi::open(profile)?;
        let pins = SystemPins::open(profile)?;
        ManualCsTransport::new(spi, pins, profile.timing)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    use super::{
        COMMAND_PREAMBLE, ManualCsTransport, Pins, READ_PREAMBLE, SpiBus, Timing, Transport,
        TransportError, WRITE_PREAMBLE, io, load_panel_profile_from_str,
    };

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Event {
        SpiWrite(Vec<u8>),
        SpiRead(usize),
        ChipSelect(bool),
        Reset(bool),
        Ready,
    }

    struct FakeSpi {
        events: Rc<RefCell<Vec<Event>>>,
        reads: VecDeque<u8>,
    }

    impl SpiBus for FakeSpi {
        fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
            self.events
                .borrow_mut()
                .push(Event::SpiWrite(bytes.to_vec()));
            Ok(())
        }

        fn read_exact(&mut self, bytes: &mut [u8]) -> io::Result<()> {
            self.events.borrow_mut().push(Event::SpiRead(bytes.len()));
            for byte in bytes {
                *byte = self.reads.pop_front().unwrap_or(0);
            }
            Ok(())
        }
    }

    struct FailingSpi {
        events: Rc<RefCell<Vec<Event>>>,
    }

    impl SpiBus for FailingSpi {
        fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
            self.events
                .borrow_mut()
                .push(Event::SpiWrite(bytes.to_vec()));
            Err(io::Error::other("injected SPI failure"))
        }

        fn read_exact(&mut self, _bytes: &mut [u8]) -> io::Result<()> {
            Err(io::Error::other("unexpected read"))
        }
    }

    struct FakePins {
        events: Rc<RefCell<Vec<Event>>>,
        ready: VecDeque<bool>,
        ready_default: bool,
    }

    impl Pins for FakePins {
        fn set_chip_select(&mut self, high: bool) -> io::Result<()> {
            self.events.borrow_mut().push(Event::ChipSelect(high));
            Ok(())
        }

        fn set_reset(&mut self, high: bool) -> io::Result<()> {
            self.events.borrow_mut().push(Event::Reset(high));
            Ok(())
        }

        fn is_ready(&mut self) -> io::Result<bool> {
            self.events.borrow_mut().push(Event::Ready);
            Ok(self.ready.pop_front().unwrap_or(self.ready_default))
        }
    }

    struct FailingReleasePins {
        events: Rc<RefCell<Vec<Event>>>,
    }

    impl Pins for FailingReleasePins {
        fn set_chip_select(&mut self, high: bool) -> io::Result<()> {
            self.events.borrow_mut().push(Event::ChipSelect(high));
            if high {
                Err(io::Error::other("injected CS release failure"))
            } else {
                Ok(())
            }
        }

        fn set_reset(&mut self, high: bool) -> io::Result<()> {
            self.events.borrow_mut().push(Event::Reset(high));
            Ok(())
        }

        fn is_ready(&mut self) -> io::Result<bool> {
            self.events.borrow_mut().push(Event::Ready);
            Ok(true)
        }
    }

    fn timing() -> Timing {
        Timing {
            ready_timeout: std::time::Duration::from_millis(5),
            ready_poll_interval: std::time::Duration::from_millis(1),
            reset_high: std::time::Duration::from_nanos(1),
            reset_low: std::time::Duration::from_nanos(1),
            reset_recovery: std::time::Duration::from_nanos(1),
        }
    }

    fn transport(
        ready_states: impl IntoIterator<Item = bool>,
        read_bytes: impl IntoIterator<Item = u8>,
    ) -> (
        ManualCsTransport<FakeSpi, FakePins>,
        Rc<RefCell<Vec<Event>>>,
    ) {
        let events = Rc::new(RefCell::new(Vec::new()));
        let spi = FakeSpi {
            events: Rc::clone(&events),
            reads: read_bytes.into_iter().collect(),
        };
        let pins = FakePins {
            events: Rc::clone(&events),
            ready: ready_states.into_iter().collect(),
            ready_default: true,
        };
        (ManualCsTransport::new(spi, pins, timing()).unwrap(), events)
    }

    #[test]
    fn command_keeps_manual_cs_asserted_across_ready_sync() {
        let (mut transport, events) = transport([true, false, true], []);

        transport.command(0x0302).unwrap();

        assert_eq!(
            *events.borrow(),
            vec![
                Event::Ready,
                Event::ChipSelect(false),
                Event::SpiWrite(COMMAND_PREAMBLE.to_vec()),
                Event::Ready,
                Event::Ready,
                Event::SpiWrite(vec![0x03, 0x02]),
                Event::ChipSelect(true),
            ]
        );
    }

    #[test]
    fn bulk_write_uses_one_preamble_and_one_cs_lifetime() {
        let (mut transport, events) = transport([true, true], []);

        transport.write_words([0x1234, 0xabcd]).unwrap();

        assert_eq!(
            *events.borrow(),
            vec![
                Event::Ready,
                Event::ChipSelect(false),
                Event::SpiWrite(WRITE_PREAMBLE.to_vec()),
                Event::Ready,
                Event::SpiWrite(vec![0x12, 0x34, 0xab, 0xcd]),
                Event::ChipSelect(true),
            ]
        );
    }

    #[test]
    fn multiword_read_consumes_dummy_and_big_endian_words() {
        let (mut transport, events) =
            transport([true, true, true], [0xaa, 0xbb, 0x12, 0x34, 0xab, 0xcd]);
        let mut words = [0_u16; 2];

        transport.read_words(&mut words).unwrap();

        assert_eq!(words, [0x1234, 0xabcd]);
        assert_eq!(
            *events.borrow(),
            vec![
                Event::Ready,
                Event::ChipSelect(false),
                Event::SpiWrite(READ_PREAMBLE.to_vec()),
                Event::Ready,
                Event::SpiRead(2),
                Event::Ready,
                Event::SpiRead(2),
                Event::SpiRead(2),
                Event::ChipSelect(true),
            ]
        );
    }

    #[test]
    fn stuck_ready_pin_times_out_without_asserting_cs() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let spi = FakeSpi {
            events: Rc::clone(&events),
            reads: VecDeque::new(),
        };
        let pins = FakePins {
            events: Rc::clone(&events),
            ready: VecDeque::new(),
            ready_default: false,
        };
        let mut transport = ManualCsTransport::new(spi, pins, timing()).unwrap();

        assert!(matches!(
            transport.command(1),
            Err(TransportError::ReadyTimeout(_))
        ));
        assert!(!events.borrow().contains(&Event::ChipSelect(false)));
    }

    #[test]
    fn shared_operation_deadline_preempts_the_longer_ready_timeout() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let spi = FakeSpi {
            events: Rc::clone(&events),
            reads: VecDeque::new(),
        };
        let pins = FakePins {
            events,
            ready: VecDeque::new(),
            ready_default: false,
        };
        let mut transport = ManualCsTransport::new(spi, pins, timing()).unwrap();
        transport.begin_operation(1);

        assert!(matches!(
            transport.command(1),
            Err(TransportError::OperationTimeout)
        ));
        transport.end_operation();
    }

    #[test]
    fn transaction_error_still_releases_manual_cs() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let spi = FailingSpi {
            events: Rc::clone(&events),
        };
        let pins = FakePins {
            events: Rc::clone(&events),
            ready: VecDeque::new(),
            ready_default: true,
        };
        let mut transport = ManualCsTransport::new(spi, pins, timing()).unwrap();

        assert!(matches!(transport.command(1), Err(TransportError::Io(_))));
        assert_eq!(
            *events.borrow(),
            vec![
                Event::Ready,
                Event::ChipSelect(false),
                Event::SpiWrite(COMMAND_PREAMBLE.to_vec()),
                Event::ChipSelect(true),
            ]
        );
    }

    #[test]
    fn transaction_and_cs_release_failures_are_both_reported() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let spi = FailingSpi {
            events: Rc::clone(&events),
        };
        let pins = FailingReleasePins {
            events: Rc::clone(&events),
        };
        let mut transport = ManualCsTransport::new(spi, pins, timing()).unwrap();

        let error = transport.command(1).unwrap_err();
        assert!(matches!(
            error,
            TransportError::TransactionAndRelease { .. }
        ));
        assert_eq!(
            *events.borrow(),
            vec![
                Event::Ready,
                Event::ChipSelect(false),
                Event::SpiWrite(COMMAND_PREAMBLE.to_vec()),
                Event::ChipSelect(true),
            ]
        );
    }

    #[test]
    fn reset_uses_high_low_high_sequence_then_waits_ready() {
        let (mut transport, events) = transport([true], []);

        transport.reset().unwrap();

        assert_eq!(
            *events.borrow(),
            vec![
                Event::ChipSelect(true),
                Event::Reset(true),
                Event::Reset(false),
                Event::Reset(true),
                Event::Ready,
            ]
        );
    }

    #[test]
    fn profile_requires_manual_cs_and_exact_safe_values() {
        let source = r#"
[[panel]]
name = "desk"
controller = "it8951"
width = 1448
height = 1072
vcom_mv = 1500
spi_device = "/dev/spidev0.0"
gpio_chip = "/dev/gpiochip0"
cs_line = 8
reset_line = 17
ready_line = 24
max_spi_hz = 1000000
ready_timeout_ms = 1000
ready_poll_us = 100
reset_high_ms = 200
reset_low_ms = 10
reset_recovery_ms = 200
display_timeout_ms = 30000
display_poll_ms = 100
"#;

        let profile = load_panel_profile_from_str(source, "desk").unwrap();

        assert_eq!(profile.cs_line, 8);
        assert_eq!(profile.vcom.get(), 1500);
        assert_eq!(profile.panel_size, paper_display::Size::new(1448, 1072));
        assert_eq!(profile.expected_firmware, None);
        assert_eq!(profile.expected_lut, None);
    }

    #[test]
    fn profile_rejects_duplicate_names_and_invalid_vcom() {
        let valid = r#"
[[panel]]
name = "desk"
controller = "it8951"
width = 4
height = 2
vcom_mv = 1500
spi_device = "/dev/spidev0.0"
gpio_chip = "/dev/gpiochip0"
cs_line = 8
reset_line = 17
ready_line = 24
max_spi_hz = 1000000
ready_timeout_ms = 1000
ready_poll_us = 100
reset_high_ms = 200
reset_low_ms = 10
reset_recovery_ms = 200
display_timeout_ms = 30000
display_poll_ms = 100
"#;
        let duplicated = format!("{valid}\n{valid}");
        assert!(matches!(
            load_panel_profile_from_str(&duplicated, "desk"),
            Err(super::ProfileError::DuplicateName(name)) if name == "desk"
        ));

        let invalid = valid.replace("vcom_mv = 1500", "vcom_mv = 0");
        assert!(matches!(
            load_panel_profile_from_str(&invalid, "desk"),
            Err(super::ProfileError::Invalid { .. })
        ));
    }

    #[test]
    fn profile_pins_controller_versions_and_caps_spi_rate() {
        let source = r#"
[[panel]]
name = "desk"
controller = "it8951"
width = 1448
height = 1072
vcom_mv = 1500
expected_firmware = "FW6"
expected_lut = "M641"
spi_device = "/dev/spidev0.0"
gpio_chip = "/dev/gpiochip0"
cs_line = 8
reset_line = 17
ready_line = 24
max_spi_hz = 12500000
ready_timeout_ms = 1000
ready_poll_us = 100
reset_high_ms = 200
reset_low_ms = 10
reset_recovery_ms = 200
display_timeout_ms = 30000
display_poll_ms = 100
"#;

        let profile = load_panel_profile_from_str(source, "desk").unwrap();
        assert_eq!(profile.expected_firmware.as_deref(), Some("FW6"));
        assert_eq!(profile.expected_lut.as_deref(), Some("M641"));

        let too_fast = source.replace("max_spi_hz = 12500000", "max_spi_hz = 12500001");
        assert!(matches!(
            load_panel_profile_from_str(&too_fast, "desk"),
            Err(super::ProfileError::Invalid { .. })
        ));

        let blank_identity = source.replace("expected_lut = \"M641\"", "expected_lut = \" \"");
        assert!(matches!(
            load_panel_profile_from_str(&blank_identity, "desk"),
            Err(super::ProfileError::Invalid { .. })
        ));
    }
}
