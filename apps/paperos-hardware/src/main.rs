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
use paper_graphics::Rotation;
#[cfg(target_os = "linux")]
use paper_it8951::Controller;
#[cfg(any(target_os = "linux", test))]
use paper_it8951::ProbeReport;
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
    /// Tests a session VCOM change after pinned identity checks; never refreshes.
    SetVcom(VcomWrite),
    /// Runs white INIT, a Gray4 GC16 calibration page, white cleanup, and sleep.
    Calibrate(Calibration),
    /// Shows the deterministic typography specimen, then cleans to white.
    Specimen(Calibration),
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
    #[command(flatten)]
    vcom: VcomAuthorization,
    /// Confirms that a visible full-screen panel refresh is intended.
    #[arg(long)]
    allow_refresh: bool,
    /// Seconds to leave the calibration page visible before white cleanup.
    #[arg(long, default_value_t = 10)]
    hold_seconds: u64,
}

#[derive(Debug, Args)]
struct VcomWrite {
    #[command(flatten)]
    hardware: HardwareSelection,
    #[command(flatten)]
    vcom: VcomAuthorization,
}

#[derive(Clone, Debug, Args)]
struct VcomAuthorization {
    /// Confirms that changing the controller's panel-sensitive VCOM is intended.
    #[arg(long = "allow-vcom-write")]
    allow_write: bool,
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
        Command::SetVcom(write) => {
            require_hardware_opt_in(&write.hardware)?;
            require_vcom_authorization(&write.vcom)?;
            let profile = load_panel_profile(&write.hardware.config, &write.hardware.profile)?;
            require_pinned_identity(&profile)?;
            run_set_vcom(&profile)
        }
        Command::Calibrate(calibration) => {
            require_hardware_opt_in(&calibration.hardware)?;
            if !calibration.allow_refresh {
                return Err(io::Error::other(
                    "calibration requires the explicit --allow-refresh flag",
                )
                .into());
            }
            require_vcom_authorization(&calibration.vcom)?;
            validate_hold_seconds(calibration.hold_seconds)?;
            let profile =
                load_panel_profile(&calibration.hardware.config, &calibration.hardware.profile)?;
            require_pinned_identity(&profile)?;
            run_calibration(&profile, Duration::from_secs(calibration.hold_seconds))
        }
        Command::Specimen(specimen) => {
            require_hardware_opt_in(&specimen.hardware)?;
            if !specimen.allow_refresh {
                return Err(io::Error::other(
                    "specimen requires the explicit --allow-refresh flag",
                )
                .into());
            }
            require_vcom_authorization(&specimen.vcom)?;
            validate_hold_seconds(specimen.hold_seconds)?;
            let profile =
                load_panel_profile(&specimen.hardware.config, &specimen.hardware.profile)?;
            require_pinned_identity(&profile)?;
            run_specimen(&profile, Duration::from_secs(specimen.hold_seconds))
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
        use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

        let requested = Arc::new(AtomicBool::new(false));
        signal_hook::flag::register(SIGHUP, Arc::clone(&requested))?;
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

fn require_vcom_authorization(authorization: &VcomAuthorization) -> Result<(), Box<dyn Error>> {
    if !authorization.allow_write {
        return Err(io::Error::other(
            "VCOM mutation requires the explicit --allow-vcom-write flag",
        )
        .into());
    }
    Ok(())
}

fn require_pinned_identity(profile: &PanelProfile) -> Result<(), Box<dyn Error>> {
    if profile.expected_firmware.is_none() || profile.expected_lut.is_none() {
        Err(io::Error::other(
            "this operation requires expected_firmware and expected_lut copied from a successful probe",
        )
        .into())
    } else {
        Ok(())
    }
}

#[cfg(any(target_os = "linux", test))]
struct SleepGuard<R, E> {
    resource: Option<R>,
    sleep: fn(&mut R) -> Result<(), E>,
    known_sleeping: bool,
}

#[cfg(any(target_os = "linux", test))]
impl<R, E> SleepGuard<R, E> {
    fn new(resource: R, sleep: fn(&mut R) -> Result<(), E>) -> Self {
        Self {
            resource: Some(resource),
            sleep,
            known_sleeping: false,
        }
    }

    fn resource_mut(&mut self) -> &mut R {
        self.resource
            .as_mut()
            .expect("sleep guard always owns its resource while armed")
    }

    #[cfg(target_os = "linux")]
    fn into_inner(mut self) -> R {
        self.resource
            .take()
            .expect("sleep guard always owns its resource while armed")
    }

    fn sleep_now(&mut self) -> Result<(), E> {
        let result = (self.sleep)(self.resource_mut());
        if result.is_ok() {
            self.known_sleeping = true;
        }
        result
    }

    #[cfg(target_os = "linux")]
    fn finish(mut self, result: Result<(), Box<dyn Error>>) -> Result<(), Box<dyn Error>>
    where
        E: std::fmt::Display + Error + 'static,
    {
        let cleanup = if self.known_sleeping {
            Ok(())
        } else {
            self.sleep_now()
        };
        self.resource.take();
        finish_with_cleanup(result, cleanup)
    }
}

#[cfg(any(target_os = "linux", test))]
impl<R, E> Drop for SleepGuard<R, E> {
    fn drop(&mut self) {
        if !self.known_sleeping
            && let Some(resource) = self.resource.as_mut()
        {
            let _ = (self.sleep)(resource);
        }
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
    let (controller, report) = open_and_probe(profile)?;
    let controller = SleepGuard::new(controller, Controller::sleep);
    print_probe(&report);
    let result = if shutdown.requested() {
        Err(interruption_error())
    } else {
        let result = verify_probe_identity(profile, &report);
        if result.is_ok() && report.current_vcom != profile.vcom {
            println!(
                "controller boot VCOM is {} mV; refresh commands will apply the panel profile target of {} mV",
                report.current_vcom.get(),
                profile.vcom.get()
            );
        }
        result
    };
    controller.finish(result)
}

#[cfg(target_os = "linux")]
fn run_set_vcom(profile: &PanelProfile) -> Result<(), Box<dyn Error>> {
    let shutdown = ShutdownSignals::install()?;
    let (controller, report) = open_and_probe(profile)?;
    let mut controller = SleepGuard::new(controller, Controller::sleep);
    print_probe(&report);
    let result = (|| {
        if shutdown.requested() {
            return Err(interruption_error());
        }
        if !should_apply_profile_vcom(profile, &report)? {
            println!(
                "VCOM already matches named profile at {} mV; no write performed",
                profile.vcom.get()
            );
            return Ok(());
        }
        controller.resource_mut().set_vcom(profile.vcom)?;
        println!(
            "VCOM changed from {} mV to {} mV for this controller session; readback verified",
            report.current_vcom.get(),
            profile.vcom.get()
        );
        Ok(())
    })();
    controller.finish(result)
}

#[cfg(not(target_os = "linux"))]
fn run_set_vcom(_profile: &PanelProfile) -> Result<(), Box<dyn Error>> {
    Err(io::Error::other("physical IT8951 access is supported only on Linux").into())
}

#[cfg(not(target_os = "linux"))]
fn run_probe(_profile: &PanelProfile) -> Result<(), Box<dyn Error>> {
    Err(io::Error::other("physical IT8951 access is supported only on Linux").into())
}

#[cfg(target_os = "linux")]
fn apply_session_vcom<T>(
    controller: &mut Controller<T>,
    profile: &PanelProfile,
    mut report: ProbeReport,
) -> Result<ProbeReport, Box<dyn Error>>
where
    T: paper_it8951::Transport,
    T::Error: std::fmt::Debug + Error + 'static,
{
    if should_apply_profile_vcom(profile, &report)? {
        controller.set_vcom(profile.vcom)?;
        println!(
            "VCOM changed from {} mV to {} mV for this controller session; readback verified",
            report.current_vcom.get(),
            profile.vcom.get()
        );
        report.current_vcom = profile.vcom;
    } else {
        println!(
            "VCOM already matches named profile at {} mV; no write performed",
            profile.vcom.get()
        );
    }
    Ok(report)
}

#[cfg(target_os = "linux")]
fn run_calibration(profile: &PanelProfile, hold: Duration) -> Result<(), Box<dyn Error>> {
    let size = profile.panel_size;
    let row_bytes = PixelFormat::Gray4
        .row_bytes(size.width)
        .ok_or_else(|| io::Error::other("panel width cannot be represented as Gray4"))?;
    let calibration = calibration_page(size)?;
    run_observed_page(
        profile,
        hold,
        "calibration",
        PixelFormat::Gray4,
        row_bytes,
        &calibration,
    )
}

#[cfg(target_os = "linux")]
fn run_specimen(profile: &PanelProfile, hold: Duration) -> Result<(), Box<dyn Error>> {
    let rotation = match profile.rotation_degrees {
        0 => Rotation::None,
        90 => Rotation::Clockwise90,
        180 => Rotation::Clockwise180,
        270 => Rotation::Clockwise270,
        _ => return Err(io::Error::other("panel profile contains an invalid rotation").into()),
    };
    let logical = paperos_specimen::render_specimen()?;
    if rotation.output_size(logical.size()) != profile.panel_size {
        return Err(io::Error::other(format!(
            "specimen rotated by {} degrees is {}×{}, but the profile expects {}×{}",
            profile.rotation_degrees,
            rotation.output_size(logical.size()).width,
            rotation.output_size(logical.size()).height,
            profile.panel_size.width,
            profile.panel_size.height
        ))
        .into());
    }
    let native = logical.rotated(rotation);
    run_observed_page(
        profile,
        hold,
        "typography specimen",
        PixelFormat::Gray8,
        native.stride_bytes(),
        native.pixels(),
    )
}

#[cfg(target_os = "linux")]
fn run_observed_page(
    profile: &PanelProfile,
    hold: Duration,
    label: &str,
    pixel_format: PixelFormat,
    stride_bytes: usize,
    page: &[u8],
) -> Result<(), Box<dyn Error>> {
    let shutdown = ShutdownSignals::install()?;
    let size = profile.panel_size;
    let buffer_len = validate_full_page(size, pixel_format, stride_bytes, page)?;
    let white = vec![0xff; buffer_len];

    let (controller, report) = open_and_probe(profile)?;
    let mut controller = SleepGuard::new(controller, Controller::sleep);
    print_probe(&report);
    let report = match apply_session_vcom(controller.resource_mut(), profile, report) {
        Ok(report) => report,
        Err(error) => return controller.finish(Err(error)),
    };
    let display =
        paper_it8951::It8951Display::new(controller.into_inner(), report, profile.display_wait);
    let mut display = SleepGuard::new(display, Display::sleep);

    let display_result = (|| -> Result<(), Box<dyn Error>> {
        if shutdown.requested() {
            return Err(interruption_error());
        }
        update_frame(
            display.resource_mut(),
            size,
            pixel_format,
            stride_bytes,
            &white,
            Waveform::Initialize,
        )?;
        if shutdown.requested() {
            return Err(interruption_error());
        }
        update_frame(
            display.resource_mut(),
            size,
            pixel_format,
            stride_bytes,
            page,
            Waveform::Grayscale,
        )?;
        if shutdown.requested() {
            return Err(interruption_error());
        }
        Ok(())
    })();
    if let Err(error) = display_result {
        return display.finish(Err(error));
    }

    if let Err(error) = display.sleep_now() {
        return display.finish(Err(error.into()));
    }
    println!(
        "controller sleeping while {label} remains visible for {} seconds",
        hold.as_secs()
    );
    wait_for_hold(hold, &shutdown.requested)?;

    let sleeping_display = display.into_inner();
    let mut controller = sleeping_display.into_controller();
    let report = match controller.probe() {
        Ok(report) => report,
        Err(error) => {
            let cleanup = controller.sleep();
            return finish_with_cleanup(
                Err(
                    io::Error::other(format!("could not reinitialize for white cleanup: {error}"))
                        .into(),
                ),
                cleanup,
            );
        }
    };
    let mut controller = SleepGuard::new(controller, Controller::sleep);
    print_probe(&report);
    let report = match apply_session_vcom(controller.resource_mut(), profile, report) {
        Ok(report) => report,
        Err(error) => return controller.finish(Err(error)),
    };
    if shutdown.requested() {
        return controller.finish(Err(interruption_error()));
    }
    let display =
        paper_it8951::It8951Display::new(controller.into_inner(), report, profile.display_wait);
    let mut display = SleepGuard::new(display, Display::sleep);

    let cleanup = update_frame(
        display.resource_mut(),
        size,
        pixel_format,
        stride_bytes,
        &white,
        Waveform::Initialize,
    );
    let cleanup = match cleanup {
        Ok(()) if shutdown.requested() => Err(interruption_error()),
        result => result,
    };
    display.finish(cleanup)
}

#[cfg(any(target_os = "linux", test))]
fn validate_full_page(
    size: Size,
    pixel_format: PixelFormat,
    stride_bytes: usize,
    page: &[u8],
) -> io::Result<usize> {
    let row_bytes = pixel_format
        .row_bytes(size.width)
        .ok_or_else(|| io::Error::other("panel width cannot represent the requested format"))?;
    if stride_bytes != row_bytes {
        return Err(io::Error::other(
            "page stride does not match its packed width",
        ));
    }
    let buffer_len = row_bytes
        .checked_mul(size.height as usize)
        .ok_or_else(|| io::Error::other("page buffer size overflow"))?;
    if page.len() != buffer_len {
        return Err(io::Error::other(format!(
            "page contains {} bytes, expected {buffer_len}",
            page.len()
        )));
    }
    Ok(buffer_len)
}

#[cfg(not(target_os = "linux"))]
fn run_calibration(_profile: &PanelProfile, _hold: Duration) -> Result<(), Box<dyn Error>> {
    Err(io::Error::other("physical IT8951 access is supported only on Linux").into())
}

#[cfg(not(target_os = "linux"))]
fn run_specimen(_profile: &PanelProfile, _hold: Duration) -> Result<(), Box<dyn Error>> {
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
fn update_frame<T>(
    display: &mut paper_it8951::It8951Display<T>,
    size: Size,
    pixel_format: PixelFormat,
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
            pixel_format,
            stride_bytes,
            pixels,
            waveform,
        })
        .map_err(Into::into)
}

#[cfg(any(target_os = "linux", test))]
fn verify_probe_identity(
    profile: &PanelProfile,
    report: &ProbeReport,
) -> Result<(), Box<dyn Error>> {
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
    verify_version(
        "firmware",
        profile.expected_firmware.as_deref(),
        &report.device_info.firmware_version,
    )?;
    verify_version(
        "LUT",
        profile.expected_lut.as_deref(),
        &report.device_info.lut_version,
    )?;
    Ok(())
}

#[cfg(any(target_os = "linux", test))]
fn should_apply_profile_vcom(
    profile: &PanelProfile,
    report: &ProbeReport,
) -> Result<bool, Box<dyn Error>> {
    require_pinned_identity(profile)?;
    verify_probe_identity(profile, report)?;
    Ok(report.current_vcom != profile.vcom)
}

#[cfg(any(target_os = "linux", test))]
fn verify_version(
    label: &str,
    expected: Option<&str>,
    observed: &[u8; 16],
) -> Result<(), Box<dyn Error>> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let observed = version_bytes(observed);
    if observed == expected.as_bytes() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "probed {label} is {:?}, but profile expects {expected:?}; refusing to refresh",
            String::from_utf8_lossy(observed)
        ))
        .into())
    }
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

#[cfg(any(target_os = "linux", test))]
fn version_string(version: &[u8; 16]) -> String {
    String::from_utf8_lossy(version_bytes(version)).into_owned()
}

#[cfg(any(target_os = "linux", test))]
fn version_bytes(version: &[u8; 16]) -> &[u8] {
    let end = version
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(version.len());
    &version[..end]
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

    let bar_top = size.height / 4;
    let bar_bottom = size.height.saturating_mul(3) / 4;
    for x in 0..size.width {
        let gray = ((u64::from(x) * 16) / u64::from(size.width)).min(15) as u8;
        for y in bar_top..bar_bottom {
            set_gray4(&mut pixels, stride, x, y, gray);
        }
    }

    if size.width >= 4 && size.height >= 3 {
        let diagnostic_bottom = (1 + (size.height / 16).max(1)).min(size.height - 1);
        for y in 1..diagnostic_bottom {
            for x in 0..size.width {
                let gray = [0, 5, 10, 15][x as usize % 4];
                set_gray4(&mut pixels, stride, x, y, gray);
            }
        }
    }

    // Draw the border last so every content pass preserves the physical edge.
    for x in 0..size.width {
        set_gray4(&mut pixels, stride, x, 0, 0);
        set_gray4(&mut pixels, stride, x, size.height - 1, 0);
    }
    for y in 0..size.height {
        set_gray4(&mut pixels, stride, 0, y, 0);
        set_gray4(&mut pixels, stride, size.width - 1, y, 0);
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
    use std::cell::Cell;
    use std::io;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    use paper_display::{PixelFormat, Size};
    use paper_it8951::{DeviceInfo, DisplayWait, ProbeReport, VcomMillivolts};
    use paper_it8951_linux::{PanelProfile, Timing};

    use super::{
        Calibration, Cli, Command, HardwareSelection, MAX_HOLD_SECONDS, SleepGuard,
        VcomAuthorization, VcomWrite, calibration_page, require_pinned_identity, run, set_gray4,
        should_apply_profile_vcom, validate_full_page, validate_hold_seconds,
        verify_probe_identity, version_string, wait_for_hold,
    };

    fn version(value: &[u8]) -> [u8; 16] {
        let mut version = [0; 16];
        version[..value.len()].copy_from_slice(value);
        version
    }

    fn profile(expected_firmware: Option<&str>, expected_lut: Option<&str>) -> PanelProfile {
        PanelProfile {
            name: "desk".to_owned(),
            panel_size: Size::new(1448, 1072),
            rotation_degrees: 0,
            vcom: VcomMillivolts::new(1_500).unwrap(),
            expected_firmware: expected_firmware.map(str::to_owned),
            expected_lut: expected_lut.map(str::to_owned),
            spi_device: PathBuf::from("/dev/spidev0.0"),
            gpio_chip: PathBuf::from("/dev/gpiochip0"),
            cs_line: 8,
            reset_line: 17,
            ready_line: 24,
            max_spi_hz: 1_000_000,
            timing: Timing {
                ready_timeout: Duration::from_secs(1),
                ready_poll_interval: Duration::from_micros(100),
                reset_high: Duration::from_millis(200),
                reset_low: Duration::from_millis(10),
                reset_recovery: Duration::from_millis(200),
            },
            display_wait: DisplayWait::new(30_000, 50).unwrap(),
        }
    }

    fn probe_report() -> ProbeReport {
        ProbeReport {
            device_info: DeviceInfo {
                panel_size: Size::new(1448, 1072),
                image_buffer_address: 0x0012_0000,
                firmware_version: version(b"FW6"),
                lut_version: version(b"M641"),
            },
            current_vcom: VcomMillivolts::new(1_500).unwrap(),
        }
    }

    struct FakeSleepResource {
        calls: Rc<Cell<u32>>,
    }

    fn fake_sleep(resource: &mut FakeSleepResource) -> Result<(), io::Error> {
        if resource.calls.get() == u32::MAX {
            return Err(io::Error::other("synthetic sleep failure"));
        }
        resource.calls.set(resource.calls.get() + 1);
        Ok(())
    }

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
        assert_eq!(&page[stride..stride + 3], &[0x05, 0xaf, 0x05]);
        assert_eq!(page[3 * stride] >> 4, 0);
        assert_eq!(page[3 * stride + stride - 1] & 0x0f, 0);
    }

    #[test]
    fn full_page_validation_rejects_stride_and_length_mismatches() {
        let size = Size::new(4, 2);
        let page = [0xff; 8];

        assert_eq!(
            validate_full_page(size, PixelFormat::Gray8, 4, &page).unwrap(),
            8
        );
        assert!(validate_full_page(size, PixelFormat::Gray8, 3, &page).is_err());
        assert!(validate_full_page(size, PixelFormat::Gray8, 4, &page[..7]).is_err());
    }

    #[test]
    fn refresh_requires_and_verifies_pinned_controller_identity() {
        let report = probe_report();
        let unpinned = profile(None, None);
        assert!(require_pinned_identity(&unpinned).is_err());
        assert!(should_apply_profile_vcom(&unpinned, &report).is_err());

        let pinned = profile(Some("FW6"), Some("M641"));
        assert!(verify_probe_identity(&pinned, &report).is_ok());

        let wrong_lut = profile(Some("FW6"), Some("M841"));
        let error = verify_probe_identity(&wrong_lut, &report).unwrap_err();
        assert!(error.to_string().contains("probed LUT"));
        assert_eq!(version_string(&report.device_info.lut_version), "M641");
    }

    #[test]
    fn profile_vcom_is_applied_only_after_identity_is_verified() {
        let pinned = profile(Some("FW6"), Some("M641"));
        let mut report = probe_report();
        report.current_vcom = VcomMillivolts::new(2_800).unwrap();

        assert!(verify_probe_identity(&pinned, &report).is_ok());
        assert!(should_apply_profile_vcom(&pinned, &report).unwrap());

        report.current_vcom = pinned.vcom;
        assert!(!should_apply_profile_vcom(&pinned, &report).unwrap());

        report.device_info.lut_version = version(b"M841");
        assert!(verify_probe_identity(&pinned, &report).is_err());
        assert!(should_apply_profile_vcom(&pinned, &report).is_err());
    }

    #[test]
    fn scope_guard_sleeps_unless_resource_is_already_known_sleeping() {
        let calls = Rc::new(Cell::new(0));
        {
            let _guard = SleepGuard::new(
                FakeSleepResource {
                    calls: Rc::clone(&calls),
                },
                fake_sleep,
            );
        }
        assert_eq!(calls.get(), 1);

        {
            let mut guard = SleepGuard::new(
                FakeSleepResource {
                    calls: Rc::clone(&calls),
                },
                fake_sleep,
            );
            guard.sleep_now().unwrap();
        }
        assert_eq!(calls.get(), 2);
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
            command: Command::SetVcom(VcomWrite {
                hardware: HardwareSelection {
                    allow_hardware: true,
                    ..selection.clone()
                },
                vcom: VcomAuthorization { allow_write: false },
            }),
        })
        .unwrap_err();
        assert!(error.to_string().contains("--allow-vcom-write"));

        let error = run(Cli {
            command: Command::Calibrate(Calibration {
                hardware: HardwareSelection {
                    allow_hardware: true,
                    ..selection.clone()
                },
                vcom: VcomAuthorization { allow_write: true },
                allow_refresh: false,
                hold_seconds: 0,
            }),
        })
        .unwrap_err();
        assert!(error.to_string().contains("--allow-refresh"));

        let error = run(Cli {
            command: Command::Specimen(Calibration {
                hardware: HardwareSelection {
                    allow_hardware: true,
                    ..selection
                },
                vcom: VcomAuthorization { allow_write: true },
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
