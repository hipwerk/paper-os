//! Deterministic in-memory display backend and dependency-free PGM export.

use core::fmt;
use std::fs;
use std::path::Path;

use paper_display::{
    Display, DisplayCapabilities, PixelFormat, Rect, Size, UpdateConstraints, UpdateRequest,
    Waveform,
};
use paper_graphics::{Framebuffer, GraphicsError, Gray8};

const FORMATS: &[PixelFormat] = &[PixelFormat::Gray8];
const WAVEFORMS: &[Waveform] = &[
    Waveform::Initialize,
    Waveform::Grayscale,
    Waveform::FastMonochrome,
];

/// Simulator update or preview failure.
#[derive(Debug)]
pub enum SimulatorError {
    /// Framebuffer creation failed.
    Graphics(GraphicsError),
    /// The requested region is empty or outside the simulated panel.
    InvalidRegion,
    /// The simulator does not accept the requested pixel encoding.
    UnsupportedPixelFormat(PixelFormat),
    /// The simulator does not advertise the requested waveform.
    UnsupportedWaveform(Waveform),
    /// Updates are rejected until the display is woken.
    Sleeping,
    /// The source does not contain every requested row.
    BufferTooShort,
    /// Preview output failed.
    Io(std::io::Error),
}

impl fmt::Display for SimulatorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Graphics(error) => error.fmt(formatter),
            Self::InvalidRegion => formatter.write_str("update region is outside the display"),
            Self::UnsupportedPixelFormat(format) => {
                write!(formatter, "simulator does not support {format:?}")
            }
            Self::UnsupportedWaveform(waveform) => {
                write!(formatter, "simulator does not support {waveform:?}")
            }
            Self::Sleeping => formatter.write_str("simulator display is sleeping"),
            Self::BufferTooShort => formatter.write_str("update pixel buffer is too short"),
            Self::Io(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SimulatorError {}

impl From<GraphicsError> for SimulatorError {
    fn from(error: GraphicsError) -> Self {
        Self::Graphics(error)
    }
}

impl From<std::io::Error> for SimulatorError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// An in-memory Gray8 display that records successful refreshes.
pub struct SimulatorDisplay {
    capabilities: DisplayCapabilities,
    frame: Framebuffer,
    updates: Vec<(Rect, Waveform)>,
    sleeping: bool,
}

impl SimulatorDisplay {
    /// Creates a white simulated display.
    pub fn new(size: Size) -> Result<Self, SimulatorError> {
        Ok(Self {
            capabilities: DisplayCapabilities {
                native_size: size,
                supported_formats: FORMATS,
                supported_waveforms: WAVEFORMS,
                partial_updates: true,
                fast_monochrome_constraints: UpdateConstraints::UNRESTRICTED,
            },
            frame: Framebuffer::new(size, Gray8::WHITE)?,
            updates: Vec::new(),
            sleeping: false,
        })
    }

    /// Returns the current simulated framebuffer.
    pub const fn frame(&self) -> &Framebuffer {
        &self.frame
    }

    /// Returns successful update regions and waveforms in order.
    pub fn updates(&self) -> &[(Rect, Waveform)] {
        &self.updates
    }

    /// Returns whether updates are currently rejected for sleep.
    pub const fn is_sleeping(&self) -> bool {
        self.sleeping
    }

    /// Writes a dependency-free PGM preview. This is a host artifact only.
    pub fn write_pgm(&self, path: impl AsRef<Path>) -> Result<(), SimulatorError> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        let header = format!(
            "P5\n{} {}\n255\n",
            self.frame.size().width,
            self.frame.size().height
        );
        let mut output = Vec::with_capacity(header.len() + self.frame.pixels().len());
        output.extend_from_slice(header.as_bytes());
        output.extend_from_slice(self.frame.pixels());
        fs::write(path, output)?;
        Ok(())
    }
}

impl Display for SimulatorDisplay {
    type Error = SimulatorError;

    fn capabilities(&self) -> &DisplayCapabilities {
        &self.capabilities
    }

    fn update(&mut self, request: UpdateRequest<'_>) -> Result<(), Self::Error> {
        if self.sleeping {
            return Err(SimulatorError::Sleeping);
        }
        if !self.capabilities.supports_format(request.pixel_format) {
            return Err(SimulatorError::UnsupportedPixelFormat(request.pixel_format));
        }
        if !self.capabilities.supports_waveform(request.waveform) {
            return Err(SimulatorError::UnsupportedWaveform(request.waveform));
        }
        if request
            .region
            .intersection(Rect::from_size(self.capabilities.native_size))
            != Some(request.region)
        {
            return Err(SimulatorError::InvalidRegion);
        }
        let required = request
            .stride_bytes
            .saturating_mul(request.region.size.height as usize);
        if request.pixels.len() < required
            || request.stride_bytes < request.region.size.width as usize
        {
            return Err(SimulatorError::BufferTooShort);
        }

        for row in 0..request.region.size.height as usize {
            let source_start = row * request.stride_bytes;
            let source_end = source_start + request.region.size.width as usize;
            let y = request.region.origin.y as usize + row;
            let destination_start =
                y * self.frame.stride_bytes() + request.region.origin.x as usize;
            let destination_end = destination_start + request.region.size.width as usize;
            self.frame.pixels_mut()[destination_start..destination_end]
                .copy_from_slice(&request.pixels[source_start..source_end]);
        }

        self.updates.push((request.region, request.waveform));
        Ok(())
    }

    fn sleep(&mut self) -> Result<(), Self::Error> {
        self.sleeping = true;
        Ok(())
    }

    fn wake(&mut self) -> Result<(), Self::Error> {
        self.sleeping = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use paper_display::{Display, PixelFormat, Rect, Size, UpdateRequest, Waveform};

    use super::{SimulatorDisplay, SimulatorError};

    #[test]
    fn partial_update_changes_only_requested_region() {
        let mut display = SimulatorDisplay::new(Size::new(4, 3)).unwrap();
        display
            .update(UpdateRequest {
                region: Rect::new(1, 1, 2, 1),
                pixel_format: PixelFormat::Gray8,
                stride_bytes: 2,
                pixels: &[0, 127],
                waveform: Waveform::Grayscale,
            })
            .unwrap();

        assert_eq!(
            display.frame().pixels(),
            &[255, 255, 255, 255, 255, 0, 127, 255, 255, 255, 255, 255]
        );
    }

    #[test]
    fn invalid_requests_do_not_change_frame_or_history() {
        let mut display = SimulatorDisplay::new(Size::new(4, 3)).unwrap();
        let original = display.frame().clone();
        let result = display.update(UpdateRequest {
            region: Rect::new(3, 2, 2, 1),
            pixel_format: PixelFormat::Gray8,
            stride_bytes: 2,
            pixels: &[0, 0],
            waveform: Waveform::Grayscale,
        });

        assert!(matches!(result, Err(SimulatorError::InvalidRegion)));
        assert_eq!(display.frame(), &original);
        assert!(display.updates().is_empty());
    }

    #[test]
    fn sleeping_display_rejects_updates_until_woken() {
        let mut display = SimulatorDisplay::new(Size::new(2, 1)).unwrap();
        display.sleep().unwrap();
        assert!(display.is_sleeping());

        let request = UpdateRequest {
            region: Rect::new(0, 0, 2, 1),
            pixel_format: PixelFormat::Gray8,
            stride_bytes: 2,
            pixels: &[0, 0],
            waveform: Waveform::Grayscale,
        };
        assert!(matches!(
            display.update(request),
            Err(SimulatorError::Sleeping)
        ));

        display.wake().unwrap();
        display.update(request).unwrap();
        assert_eq!(display.updates().len(), 1);
    }

    #[test]
    fn unsupported_format_and_waveform_are_rejected() {
        let mut display = SimulatorDisplay::new(Size::new(2, 1)).unwrap();
        let unsupported_format = display.update(UpdateRequest {
            region: Rect::new(0, 0, 2, 1),
            pixel_format: PixelFormat::Monochrome1,
            stride_bytes: 1,
            pixels: &[0],
            waveform: Waveform::FastMonochrome,
        });
        assert!(matches!(
            unsupported_format,
            Err(SimulatorError::UnsupportedPixelFormat(
                PixelFormat::Monochrome1
            ))
        ));

        let unsupported_waveform = display.update(UpdateRequest {
            region: Rect::new(0, 0, 2, 1),
            pixel_format: PixelFormat::Gray8,
            stride_bytes: 2,
            pixels: &[0, 0],
            waveform: Waveform::ControllerSpecific(99),
        });
        assert!(matches!(
            unsupported_waveform,
            Err(SimulatorError::UnsupportedWaveform(
                Waveform::ControllerSpecific(99)
            ))
        ));
    }

    #[test]
    fn short_rows_are_rejected() {
        let mut display = SimulatorDisplay::new(Size::new(2, 2)).unwrap();
        let result = display.update(UpdateRequest {
            region: Rect::new(0, 0, 2, 2),
            pixel_format: PixelFormat::Gray8,
            stride_bytes: 2,
            pixels: &[0, 0],
            waveform: Waveform::Grayscale,
        });
        assert!(matches!(result, Err(SimulatorError::BufferTooShort)));
    }
}
