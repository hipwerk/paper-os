#![no_std]

use core::fmt::Debug;

/// A two-dimensional size in physical pixels.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub const fn pixel_count(self) -> Option<usize> {
        match self.width.checked_mul(self.height) {
            Some(value) => Some(value as usize),
            None => None,
        }
    }
}

/// A point in physical pixel coordinates.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}

impl Point {
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

/// A half-open rectangle: `[x, x + width) × [y, y + height)`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(width, height),
        }
    }

    pub const fn from_size(size: Size) -> Self {
        Self::new(0, 0, size.width, size.height)
    }

    pub const fn is_empty(self) -> bool {
        self.size.is_empty()
    }

    pub const fn right(self) -> u32 {
        self.origin.x.saturating_add(self.size.width)
    }

    pub const fn bottom(self) -> u32 {
        self.origin.y.saturating_add(self.size.height)
    }

    pub const fn contains(self, point: Point) -> bool {
        point.x >= self.origin.x
            && point.y >= self.origin.y
            && point.x < self.right()
            && point.y < self.bottom()
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let x = self.origin.x.max(other.origin.x);
        let y = self.origin.y.max(other.origin.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());

        (right > x && bottom > y).then(|| Self::new(x, y, right - x, bottom - y))
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }

        let x = self.origin.x.min(other.origin.x);
        let y = self.origin.y.min(other.origin.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Self::new(x, y, right - x, bottom - y)
    }
}

/// Pixel encodings accepted at the display boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    Monochrome1,
    Gray2,
    Gray4,
    Gray8,
}

impl PixelFormat {
    pub const fn bits_per_pixel(self) -> u8 {
        match self {
            Self::Monochrome1 => 1,
            Self::Gray2 => 2,
            Self::Gray4 => 4,
            Self::Gray8 => 8,
        }
    }
}

/// Semantic waveform intent. A controller backend maps this to its firmware LUT.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Waveform {
    /// Clear accumulated charge and return the panel to a known state.
    Initialize,
    /// Highest-quality grayscale update.
    Grayscale,
    /// Fast black/white update; ghosting is expected to accumulate.
    FastMonochrome,
    /// A backend-specific waveform requested by an expert caller.
    ControllerSpecific(u16),
}

/// Physical panel rotation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Rotation {
    #[default]
    Degrees0,
    Degrees90,
    Degrees180,
    Degrees270,
}

/// Region restrictions for a format/waveform combination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UpdateConstraints {
    pub x_alignment: u32,
    pub y_alignment: u32,
    pub width_alignment: u32,
    pub height_alignment: u32,
}

impl UpdateConstraints {
    pub const UNRESTRICTED: Self = Self::new(1, 1, 1, 1);

    pub const fn new(
        x_alignment: u32,
        y_alignment: u32,
        width_alignment: u32,
        height_alignment: u32,
    ) -> Self {
        Self {
            x_alignment,
            y_alignment,
            width_alignment,
            height_alignment,
        }
    }

    /// Expands a region outward to legal boundaries, clipped to the panel.
    pub fn align_region(self, region: Rect, panel: Size) -> Rect {
        let x_alignment = self.x_alignment.max(1);
        let y_alignment = self.y_alignment.max(1);
        let width_alignment = self.width_alignment.max(1);
        let height_alignment = self.height_alignment.max(1);

        let x = region.origin.x - (region.origin.x % x_alignment);
        let y = region.origin.y - (region.origin.y % y_alignment);
        let wanted_width = align_up(region.right().saturating_sub(x), width_alignment);
        let wanted_height = align_up(region.bottom().saturating_sub(y), height_alignment);
        let legal_width =
            panel.width.saturating_sub(x) - (panel.width.saturating_sub(x) % width_alignment);
        let legal_height =
            panel.height.saturating_sub(y) - (panel.height.saturating_sub(y) % height_alignment);

        Rect::new(
            x,
            y,
            wanted_width.min(legal_width),
            wanted_height.min(legal_height),
        )
    }
}

const fn align_up(value: u32, alignment: u32) -> u32 {
    let remainder = value % alignment;
    if remainder == 0 {
        value
    } else {
        value.saturating_add(alignment - remainder)
    }
}

/// Immutable capabilities discovered from a display/controller pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayCapabilities {
    pub native_size: Size,
    pub supported_formats: &'static [PixelFormat],
    pub supported_waveforms: &'static [Waveform],
    pub partial_updates: bool,
    pub fast_monochrome_constraints: UpdateConstraints,
}

/// One intentional display update.
#[derive(Clone, Copy, Debug)]
pub struct UpdateRequest<'a> {
    pub region: Rect,
    pub pixel_format: PixelFormat,
    pub stride_bytes: usize,
    pub pixels: &'a [u8],
    pub waveform: Waveform,
}

/// A controller-agnostic physical display.
///
/// Implementations own upload, refresh completion, and power-state details.
/// The runtime owns policy: deciding *when* and *what* to update.
pub trait Display {
    type Error: Debug;

    fn capabilities(&self) -> &DisplayCapabilities;

    fn update(&mut self, request: UpdateRequest<'_>) -> Result<(), Self::Error>;

    fn sleep(&mut self) -> Result<(), Self::Error>;

    fn wake(&mut self) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::{Rect, Size, UpdateConstraints};

    #[test]
    fn intersection_uses_half_open_coordinates() {
        let left = Rect::new(0, 0, 10, 10);
        let right = Rect::new(9, 5, 5, 10);

        assert_eq!(left.intersection(right), Some(Rect::new(9, 5, 1, 5)));
        assert_eq!(left.intersection(Rect::new(10, 0, 1, 1)), None);
    }

    #[test]
    fn aligns_fast_update_outward_and_clips_to_panel() {
        let constraints = UpdateConstraints::new(32, 1, 32, 1);
        let panel = Size::new(1448, 1072);

        assert_eq!(
            constraints.align_region(Rect::new(31, 10, 40, 20), panel),
            Rect::new(0, 10, 96, 20)
        );
        assert_eq!(
            constraints.align_region(Rect::new(1440, 10, 8, 20), panel),
            Rect::new(1440, 10, 0, 20)
        );
    }
}
