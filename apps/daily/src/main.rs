use std::env;
use std::error::Error;
use std::path::PathBuf;

use paper_display::{Display, PixelFormat, Rect, Size, UpdateRequest, Waveform};
use paper_graphics::{Framebuffer, Gray8};
use paper_simulator::SimulatorDisplay;

const DISPLAY_SIZE: Size = Size::new(1448, 1072);

fn main() -> Result<(), Box<dyn Error>> {
    let output = env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("artifacts/daily.pgm"), PathBuf::from);

    let frame = render_placeholder()?;
    let mut display = SimulatorDisplay::new(DISPLAY_SIZE)?;
    display.update(UpdateRequest {
        region: Rect::from_size(DISPLAY_SIZE),
        pixel_format: PixelFormat::Gray8,
        stride_bytes: frame.stride_bytes(),
        pixels: frame.pixels(),
        waveform: Waveform::Grayscale,
    })?;
    display.write_pgm(&output)?;

    println!("wrote PaperOS preview to {}", output.display());
    Ok(())
}

fn render_placeholder() -> Result<Framebuffer, Box<dyn Error>> {
    let mut frame = Framebuffer::new(DISPLAY_SIZE, Gray8::WHITE)?;
    frame.fill_rect(Rect::new(64, 64, 1320, 8), Gray8::BLACK);
    frame.stroke_rect(Rect::new(64, 120, 640, 360), 4, Gray8::BLACK);
    frame.fill_rect(Rect::new(96, 160, 400, 40), Gray8(48));
    frame.fill_rect(Rect::new(96, 224, 520, 20), Gray8(150));
    frame.stroke_rect(Rect::new(736, 120, 648, 824), 4, Gray8::BLACK);
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use paper_display::{Point, Size};
    use paper_graphics::Gray8;

    use super::{DISPLAY_SIZE, render_placeholder};

    #[test]
    fn placeholder_is_deterministic_and_panel_sized() {
        let frame = render_placeholder().unwrap();
        assert_eq!(frame.size(), Size::new(1448, 1072));
        assert_eq!(frame.get(Point::new(64, 64)), Some(Gray8::BLACK));
        assert_eq!(frame.get(Point::new(0, 0)), Some(Gray8::WHITE));
        assert_eq!(DISPLAY_SIZE, frame.size());
    }
}
