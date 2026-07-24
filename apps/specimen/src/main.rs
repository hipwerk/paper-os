//! Desktop preview entry point for the deterministic typography specimen.

use std::env;
use std::error::Error;
use std::path::PathBuf;

use paper_display::{Display, PixelFormat, Rect, UpdateRequest, Waveform};
use paper_simulator::SimulatorDisplay;
use paperos_specimen::{SPECIMEN_SIZE, render_specimen};

fn main() -> Result<(), Box<dyn Error>> {
    let output = env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("artifacts/specimen.pgm"), PathBuf::from);
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }

    let frame = render_specimen()?;
    let mut display = SimulatorDisplay::new(SPECIMEN_SIZE)?;
    display.update(UpdateRequest {
        region: Rect::from_size(SPECIMEN_SIZE),
        pixel_format: PixelFormat::Gray8,
        stride_bytes: frame.stride_bytes(),
        pixels: frame.pixels(),
        waveform: Waveform::Grayscale,
    })?;
    display.write_pgm(&output)?;

    println!("wrote PaperOS typography specimen to {}", output.display());
    Ok(())
}
