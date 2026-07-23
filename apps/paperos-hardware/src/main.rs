//! Explicit, fail-closed IT8951 bring-up commands.

use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;
#[cfg(target_os = "linux")]
use std::sync::Arc;
#[cfg(any(target_os = "linux", test))]
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
#[cfg(any(target_os = "linux", test))]
use std::time::Instant;

use clap::{Args, Parser, Subcommand};
#[cfg(target_os = "linux")]
use paper_display::{Display, Rect, UpdateRequest, Waveform};
#[cfg(any(target_os = "linux", test))]
use paper_display::{PixelFormat, Size};
#[cfg(target_os = "linux")]
use paper_it8951::{Controller, ProbeReport};
use paper_it8951_linux::{PanelProfile, load_panel_profile};

const MAX_HOLD_SECONDS: u64 = 120;
#[cfg(any(target_os = "linux", test))]
const HOLD_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Parser)]
#[command(
    name = "paperos-hardware",
    about = "PaperOS hardware bring-up diagnostic"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Verifies that the diagnostic executable starts without opening devices.
    SelfTest,
    /// Resets, wakes, reads identity and VCOM, then sleeps; never refreshes.
    Probe(HardwareSelection),
    /// Runs white INIT, a Gray4 GC16 calibration page, white cleanup, and sleep.
    Calibrate(Calibration),
}

#[derive(Clone, Debug, Args)]
struct HardwareSelection {
    /// Local TOML file containing the named physical panel.
    #[arg(long, default_value = "hardware/panels.local.toml")]
    config: PathBuf,
    /// Exact panel fixture name; the first detected device is never selected.
    #[arg(long)]
    profile: String,
    /// Confirms that opening and resetting the configured physical controller is intended.
    #[arg(long)]
    allow_hardware: bool,
}

#[derive(Debug, Args)]
struct Calibration {
    #[command(flatten)]
    hardware: HardwareSelection,
    /// Confirms that a visible full-screen panel refresh is intended.
    #[arg(long)]
    allow_refresh: bool,
    /// Seconds to leave the calibration page visible before white cleanup.
    #[arg(long, default_value_t = 10)]
    hold_seconds: u64,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("paperos-hardware: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    match cli.command {
        Command::SelfTest => {
            println!("paperos-hardware self-test passed; no devices were opened");
            Ok(())
        }
        Command::Probe(selection) => {
            require_hardware_opt_in(&selection)?;
            let profile = load_panel_profile(&selection.config, &selection.profile)?;
            run_probe(&profile)
        }
        Command::Calibrate(calibration) => {
            require_hardware_opt_in(&calibration.hardware)?;
            if !calibration.allow_refresh {
                return Err(io::Error::other(
                    "calibration requires the explicit --allow-refresh flag",
                )
                .into());
            }
            validate_hold_seconds(calibration.hold_seconds)?;
            let profile =
                load_panel_profile(&calibration.hardware.config, &calibration.hardware.profile)?;
            run_calibration(&profile, Duration::from_secs(calibration.hold_seconds))
        }
    }
}

fn validate_hold_seconds(seconds: u64) -> Result<(), Box<dyn Error>> {
    if seconds <= MAX_HOLD_SECONDS {
        Ok(())
    } else {
        Err(io::Error::other(format!("--hold-seconds must be at most {MAX_HOLD_SECONDS}")).into())
    }
}

#[cfg(target_os = "linux")]
struct ShutdownSignals {
    requested: Arc<AtomicBool>,
}

#[cfg(target_os = "linux")]
impl ShutdownSignals {
    fn install() -> io::Result<Self> {
        use signal_hook::consts::{SIGINT, SIGTERM};

        let requested = Arc::new(AtomicBool::new(false));
        signal_hook::flag::register(SIGINT, Arc::clone(&requested))?;
        signal_hook::flag::register(SIGTERM, Arc::clone(&requested))?;
        Ok(Self { requested })
    }

    fn requested(&self) -> bool {
        self.requested.load(Ordering::Relaxed)
    }
}

#[cfg(any(target_os = "linux", test))]
fn wait_for_hold(hold: Duration, interrupted: &AtomicBool) -> io::Result<()> {
    let deadline = Instant::now() + hold;
    loop {
        if interrupted.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "calibration interrupted; controller is asleep and the calibration page remains visible",
            ));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }
        std::thread::sleep(remaining.min(HOLD_POLL_INTERVAL));
    }
}

#[cfg(target_os = "linux")]
fn interruption_error() -> Box<dyn Error> {
    io::Error::new(
        io::ErrorKind::Interrupted,
        "hardware operation interrupted; controller sleep was requested",
    )
    .into()
}

fn require_hardware_opt_in(selection: &HardwareSelection) -> Result<(), Box<dyn Error>> {
    if selection.allow_hardware {
        Ok(())
    } else {
        Err(io::Error::other("physical access requires the explicit --allow-hardware flag").into())
    }
}

#[cfg(target_os = "linux")]
fn open_and_probe(
    profile: &PanelProfile,
) -> Result<
    (
        Controller<paper_it8951_linux::system::SystemTransport>,
        ProbeReport,
    ),
    Box<dyn Error>,
> {
    let transport = paper_it8951_linux::system::open(profile)?;
    let mut controller = Controller::new(transport);
    match controller.probe() {
        Ok(report) => Ok((controller, report)),
        Err(probe_error) => {
            let sleep_error = controller.sleep().err();
            match sleep_error {
                Some(sleep_error) => Err(io::Error::other(format!(
                    "probe failed: {probe_error}; best-effort sleep also failed: {sleep_error}"
                ))
                .into()),
                None => Err(probe_error.into()),
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn run_probe(profile: &PanelProfile) -> Result<(), Box<dyn Error>> {
    let shutdown = ShutdownSignals::install()?;
    let (mut controller, report) = open_and_probe(profile)?;
    print_probe(&report);
    let result = if shutdown.requested() {
        Err(interruption_error())
    } else {
        verify_probe(profile, &report)
    };
    let sleep = controller.sleep();
    finish_with_cleanup(result, sleep)
}

#[cfg(not(target_os = "linux"))]
fn run_probe(_profile: &PanelProfile) -> Result<(), Box<dyn Error>> {
    Err(io::Error::other("physical IT8951 access is supported only on Linux").into())
}

#[cfg(target_os = "linux")]
fn run_calibration(profile: &PanelProfile, hold: Duration) -> Result<(), Box<dyn Error>> {
    let shutdown = ShutdownSignals::install()?;
    let size = profile.panel_size;
    let row_bytes = PixelFormat::Gray4
        .row_bytes(size.width)
        .ok_or_else(|| io::Error::other("panel width cannot be represented as Gray4"))?;
    let buffer_len = row_bytes
        .checked_mul(size.height as usize)
        .ok_or_else(|| io::Error::other("calibration buffer size overflow"))?;
    let white = vec![0xff; buffer_len];
    let calibration = calibration_page(size)?;

    let (mut controller, report) = open_and_probe(profile)?;
    print_probe(&report);
    if let Err(error) = verify_probe(profile, &report) {
        return finish_with_cleanup(Err(error), controller.sleep());
    }
    let mut display = paper_it8951::It8951Display::new(controller, report, profile.display_wait);

    let display_result = (|| -> Result<(), Box<dyn Error>> {
        if shutdown.requested() {
            return Err(interruption_error());
        }
        update_gray4(&mut display, size, row_bytes, &white, Waveform::Initialize)?;
        if shutdown.requested() {
            return Err(interruption_error());
        }
        update_gray4(
            &mut display,
            size,
            row_bytes,
            &calibration,
            Waveform::Grayscale,
        )?;
        if shutdown.requested() {
            return Err(interruption_error());
        }
        Ok(())
    })();
    if let Err(error) = display_result {
        return finish_with_cleanup(Err(error), display.sleep());
    }

    if let Err(error) = display.sleep() {
        return finish_with_cleanup(Err(error.into()), display.sleep());
    }
    println!(
        "controller sleeping while calibration page remains visible for {} seconds",
        hold.as_secs()
    );
    wait_for_hold(hold, &shutdown.requested)?;

    if let Err(error) = display.wake() {
        let error: Box<dyn Error> =
            io::Error::other(format!("could not reinitialize for white cleanup: {error}")).into();
        return finish_with_cleanup(Err(error), display.sleep());
    }
    if shutdown.requested() {
        return finish_with_cleanup(Err(interruption_error()), display.sleep());
    }

    let cleanup = update_gray4(&mut display, size, row_bytes, &white, Waveform::Initialize);
    let cleanup = match cleanup {
        Ok(()) if shutdown.requested() => Err(interruption_error()),
        result => result,
    };
    finish_with_cleanup(cleanup, display.sleep())
}

#[cfg(not(target_os = "linux"))]
fn run_calibration(_profile: &PanelProfile, _hold: Duration) -> Result<(), Box<dyn Error>> {
    Err(io::Error::other("physical IT8951 access is supported only on Linux").into())
}

#[cfg(target_os = "linux")]
fn finish_with_cleanup<E>(
    result: Result<(), Box<dyn Error>>,
    cleanup: Result<(), E>,
) -> Result<(), Box<dyn Error>>
where
    E: std::fmt::Display + Error + 'static,
{
    match (result, cleanup) {
        (Err(error), Err(cleanup_error)) => Err(io::Error::other(format!(
            "{error}; best-effort sleep also failed: {cleanup_error}"
        ))
        .into()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error.into()),
        (Ok(()), Ok(())) => Ok(()),
    }
}

#[cfg(target_os = "linux")]
fn update_gray4<T>(
    display: &mut paper_it8951::It8951Display<T>,
    size: Size,
    stride_bytes: usize,
    pixels: &[u8],
    waveform: Waveform,
) -> Result<(), Box<dyn Error>>
where
    T: paper_it8951::Transport,
    T::Error: std::fmt::Debug + Error + 'static,
{
    display
        .update(UpdateRequest {
            region: Rect::from_size(size),
            pixel_format: PixelFormat::Gray4,
            stride_bytes,
            pixels,
            waveform,
        })
        .map_err(Into::into)
}

#[cfg(target_os = "linux")]
fn verify_probe(profile: &PanelProfile, report: &ProbeReport) -> Result<(), Box<dyn Error>> {
    if report.device_info.panel_size != profile.panel_size {
        return Err(io::Error::other(format!(
            "probed panel is {}×{}, but profile expects {}×{}",
            report.device_info.panel_size.width,
            report.device_info.panel_size.height,
            profile.panel_size.width,
            profile.panel_size.height
        ))
        .into());
    }
    if report.current_vcom != profile.vcom {
        return Err(io::Error::other(format!(
            "probed VCOM is {} mV, but named profile requires {} mV; refusing to refresh",
            report.current_vcom.get(),
            profile.vcom.get()
        ))
        .into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn print_probe(report: &ProbeReport) {
    println!(
        "panel={}×{} image_buffer=0x{:08x} firmware={} lut={} vcom_mv={}",
        report.device_info.panel_size.width,
        report.device_info.panel_size.height,
        report.device_info.image_buffer_address,
        version_string(&report.device_info.firmware_version),
        version_string(&report.device_info.lut_version),
        report.current_vcom.get()
    );
}

#[cfg(target_os = "linux")]
fn version_string(version: &[u8; 16]) -> String {
    let end = version
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(version.len());
    String::from_utf8_lossy(&version[..end]).into_owned()
}

#[cfg(any(target_os = "linux", test))]
fn calibration_page(size: Size) -> Result<Vec<u8>, Box<dyn Error>> {
    let stride = PixelFormat::Gray4
        .row_bytes(size.width)
        .ok_or_else(|| io::Error::other("calibration width overflow"))?;
    if stride % 2 != 0 || size.is_empty() {
        return Err(io::Error::other(
            "calibration requires non-empty dimensions with four-pixel row alignment",
        )
        .into());
    }
    let len = stride
        .checked_mul(size.height as usize)
        .ok_or_else(|| io::Error::other("calibration buffer size overflow"))?;
    let mut pixels = vec![0xff; len];

    for x in 0..size.width {
        set_gray4(&mut pixels, stride, x, 0, 0);
        set_gray4(&mut pixels, stride, x, size.height - 1, 0);
    }
    for y in 0..size.height {
        set_gray4(&mut pixels, stride, 0, y, 0);
        set_gray4(&mut pixels, stride, size.width - 1, y, 0);
    }

    let bar_top = size.height / 4;
    let bar_bottom = size.height.saturating_mul(3) / 4;
    for x in 0..size.width {
        let gray = ((u64::from(x) * 16) / u64::from(size.width)).min(15) as u8;
        for y in bar_top..bar_bottom {
            set_gray4(&mut pixels, stride, x, y, gray);
        }
    }
    Ok(pixels)
}

#[cfg(any(target_os = "linux", test))]
fn set_gray4(pixels: &mut [u8], stride: usize, x: u32, y: u32, gray: u8) {
    let index = y as usize * stride + x as usize / 2;
    let value = gray.min(15);
    if x.is_multiple_of(2) {
        pixels[index] = (pixels[index] & 0x0f) | (value << 4);
    } else {
        pixels[index] = (pixels[index] & 0xf0) | value;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    use paper_display::{PixelFormat, Size};

    use super::{
        Calibration, Cli, Command, HardwareSelection, MAX_HOLD_SECONDS, calibration_page, run,
        set_gray4, validate_hold_seconds, wait_for_hold,
    };

    #[test]
    fn gray4_packing_places_left_pixel_in_high_nibble() {
        let mut pixels = [0xff];
        set_gray4(&mut pixels, 1, 0, 0, 2);
        set_gray4(&mut pixels, 1, 1, 0, 13);
        assert_eq!(pixels, [0x2d]);
    }

    #[test]
    fn calibration_page_has_border_and_tonal_range() {
        let size = Size::new(16, 8);
        let page = calibration_page(size).unwrap();
        let stride = PixelFormat::Gray4.row_bytes(size.width).unwrap();

        assert_eq!(page.len(), stride * size.height as usize);
        assert_eq!(page[0] >> 4, 0);
        assert_eq!(page[(size.height as usize - 1) * stride] >> 4, 0);
        assert_ne!(page[3 * stride + 1], page[3 * stride + stride - 2]);
    }

    #[test]
    fn physical_commands_require_separate_explicit_opt_ins() {
        let selection = HardwareSelection {
            config: "missing.toml".into(),
            profile: "desk".to_owned(),
            allow_hardware: false,
        };
        let error = run(Cli {
            command: Command::Probe(selection.clone()),
        })
        .unwrap_err();
        assert!(error.to_string().contains("--allow-hardware"));

        let error = run(Cli {
            command: Command::Calibrate(Calibration {
                hardware: HardwareSelection {
                    allow_hardware: true,
                    ..selection
                },
                allow_refresh: false,
                hold_seconds: 0,
            }),
        })
        .unwrap_err();
        assert!(error.to_string().contains("--allow-refresh"));
    }

    #[test]
    fn calibration_hold_is_bounded_and_interruptible() {
        assert!(validate_hold_seconds(MAX_HOLD_SECONDS).is_ok());
        assert!(validate_hold_seconds(MAX_HOLD_SECONDS + 1).is_err());

        let interrupted = AtomicBool::new(true);
        let error = wait_for_hold(Duration::from_secs(1), &interrupted).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
    }
}
