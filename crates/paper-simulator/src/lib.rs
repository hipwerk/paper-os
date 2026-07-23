//! Deterministic in-memory display backend and dependency-free PGM export.

use core::fmt;
use std::fs;
use std::path::Path;

use paper_display::{
    Display, DisplayCapabilities, PixelFormat, Rect, Size, UpdateConstraints, UpdateProfile,
    UpdateRequest, Waveform,
};
use paper_graphics::{Framebuffer, GraphicsError, Gray8};

const PROFILES: &[UpdateProfile] = &[
    UpdateProfile::new(
        PixelFormat::Gray8,
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
];

/// Simulator update or preview failure.
#[derive(Debug)]
pub enum SimulatorError {
    /// Framebuffer creation failed.
    Graphics(GraphicsError),
    /// The region is empty, outside the panel, or illegal for its profile.
    InvalidRegion,
    /// The simulator does not advertise the requested format/waveform pair.
    UnsupportedUpdateProfile {
        /// Requested controller-bound encoding.
        pixel_format: PixelFormat,
        /// Requested semantic waveform.
        waveform: Waveform,
    },
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
            Self::UnsupportedUpdateProfile {
                pixel_format,
                waveform,
            } => write!(
                formatter,
                "simulator does not support {pixel_format:?} with {waveform:?}"
            ),
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
                update_profiles: PROFILES,
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
        let profile = self
            .capabilities
            .profile(request.pixel_format, request.waveform)
            .ok_or(SimulatorError::UnsupportedUpdateProfile {
                pixel_format: request.pixel_format,
                waveform: request.waveform,
            })?;
        if request
            .region
            .intersection(Rect::from_size(self.capabilities.native_size))
            != Some(request.region)
        {
            return Err(SimulatorError::InvalidRegion);
        }
        let full_panel = Rect::from_size(self.capabilities.native_size);
        if (request.region != full_panel && !profile.supports_partial())
            || profile
                .constraints()
                .align_region(request.region, self.capabilities.native_size)
                != request.region
        {
            return Err(SimulatorError::InvalidRegion);
        }
        let row_width = request.region.size.width as usize;
        let preceding_rows = request.region.size.height.saturating_sub(1) as usize;
        let required = request
            .stride_bytes
            .saturating_mul(preceding_rows)
            .saturating_add(row_width);
        if request.pixels.len() < required || request.stride_bytes < row_width {
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
    use paper_display::{Display, PixelFormat, Point, Rect, Size, UpdateRequest, Waveform};
    use paper_graphics::{Framebuffer, Gray8};
    use paper_runtime::{RefreshPlan, RefreshPolicy, RefreshRuntime};

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
    fn runtime_plan_uses_a_profile_the_simulator_executes() {
        let size = Size::new(4, 3);
        let previous = Framebuffer::new(size, Gray8::WHITE).unwrap();
        let mut next = previous.clone();
        next.set(Point::new(1, 1), Gray8::BLACK);
        let mut runtime =
            RefreshRuntime::from_known_panel_state(previous, 0, RefreshPolicy::default()).unwrap();
        let mut display = SimulatorDisplay::new(size).unwrap();
        let pending = runtime.plan(next, display.capabilities()).unwrap();

        let RefreshPlan::Partial {
            region,
            pixel_format,
            waveform,
            ..
        } = pending.plan()
        else {
            panic!("one changed pixel should produce a partial simulator update");
        };
        let stride = pending.framebuffer().stride_bytes();
        let source_start = region.origin.y as usize * stride + region.origin.x as usize;
        display
            .update(UpdateRequest {
                region,
                pixel_format,
                stride_bytes: stride,
                pixels: &pending.framebuffer().pixels()[source_start..],
                waveform,
            })
            .unwrap();
        runtime.commit_success(pending).unwrap();

        assert_eq!(display.frame(), runtime.previous_frame());
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
            Err(SimulatorError::UnsupportedUpdateProfile {
                pixel_format: PixelFormat::Monochrome1,
                waveform: Waveform::FastMonochrome,
            })
        ));

        let unsupported_waveform = display.update(UpdateRequest {
            region: Rect::new(0, 0, 2, 1),
            pixel_format: PixelFormat::Gray8,
            stride_bytes: 2,
            pixels: &[0, 0],
            waveform: Waveform::FastMonochrome,
        });
        assert!(matches!(
            unsupported_waveform,
            Err(SimulatorError::UnsupportedUpdateProfile {
                pixel_format: PixelFormat::Gray8,
                waveform: Waveform::FastMonochrome,
            })
        ));
    }

    #[test]
    fn full_only_profile_rejects_bounded_update() {
        let mut display = SimulatorDisplay::new(Size::new(2, 2)).unwrap();
        let result = display.update(UpdateRequest {
            region: Rect::new(0, 0, 1, 1),
            pixel_format: PixelFormat::Gray8,
            stride_bytes: 1,
            pixels: &[0],
            waveform: Waveform::Initialize,
        });

        assert!(matches!(result, Err(SimulatorError::InvalidRegion)));
        assert!(display.updates().is_empty());
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

    #[test]
    fn final_source_row_requires_only_its_visible_pixels() {
        let mut display = SimulatorDisplay::new(Size::new(4, 3)).unwrap();
        display
            .update(UpdateRequest {
                region: Rect::new(3, 2, 1, 1),
                pixel_format: PixelFormat::Gray8,
                stride_bytes: 4,
                pixels: &[0],
                waveform: Waveform::Grayscale,
            })
            .unwrap();

        assert_eq!(display.frame().get(Point::new(3, 2)), Some(Gray8::BLACK));
    }
}
