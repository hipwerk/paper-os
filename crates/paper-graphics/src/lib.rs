//! Deterministic Gray8 framebuffer storage and drawing primitives.

use core::fmt;

use paper_display::{Point, Rect, Size};

/// An eight-bit grayscale value where 0 is black and 255 is white.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Gray8(pub u8);

impl Gray8 {
    /// Fully black ink.
    pub const BLACK: Self = Self(0);
    /// Fully white background.
    pub const WHITE: Self = Self(255);
}

/// Framebuffer construction failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphicsError {
    /// Dimensions are empty or their pixel count overflows.
    InvalidSize(Size),
    /// A provided pixel vector does not match the framebuffer dimensions.
    BufferLength {
        /// Required number of Gray8 pixels.
        expected: usize,
        /// Supplied number of Gray8 pixels.
        actual: usize,
    },
}

impl fmt::Display for GraphicsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSize(size) => write!(
                formatter,
                "framebuffer size {}×{} is empty or overflows address space",
                size.width, size.height
            ),
            Self::BufferLength { expected, actual } => {
                write!(formatter, "expected {expected} pixels, received {actual}")
            }
        }
    }
}

impl std::error::Error for GraphicsError {}

/// The canonical render target.
///
/// PaperOS renders to Gray8 for predictable typography and converts to a
/// controller-specific packed format only at the display boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Framebuffer {
    size: Size,
    pixels: Vec<u8>,
}

impl Framebuffer {
    /// Allocates a framebuffer filled with `background`.
    pub fn new(size: Size, background: Gray8) -> Result<Self, GraphicsError> {
        let len = size
            .pixel_count()
            .filter(|_| !size.is_empty())
            .ok_or(GraphicsError::InvalidSize(size))?;

        Ok(Self {
            size,
            pixels: vec![background.0; len],
        })
    }

    /// Wraps an exactly sized Gray8 pixel vector.
    pub fn from_pixels(size: Size, pixels: Vec<u8>) -> Result<Self, GraphicsError> {
        let expected = size
            .pixel_count()
            .filter(|_| !size.is_empty())
            .ok_or(GraphicsError::InvalidSize(size))?;
        if pixels.len() != expected {
            return Err(GraphicsError::BufferLength {
                expected,
                actual: pixels.len(),
            });
        }

        Ok(Self { size, pixels })
    }

    /// Returns physical framebuffer dimensions.
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Returns the byte distance between rows.
    pub const fn stride_bytes(&self) -> usize {
        self.size.width as usize
    }

    /// Returns immutable canonical Gray8 pixels in row-major order.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Returns mutable canonical Gray8 pixels in row-major order.
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    /// Fills the complete framebuffer.
    pub fn clear(&mut self, color: Gray8) {
        self.pixels.fill(color.0);
    }

    /// Returns one pixel or `None` outside the framebuffer.
    pub fn get(&self, point: Point) -> Option<Gray8> {
        self.index(point).map(|index| Gray8(self.pixels[index]))
    }

    /// Draws a pixel, returning `false` when it falls outside the framebuffer.
    pub fn set(&mut self, point: Point, color: Gray8) -> bool {
        let Some(index) = self.index(point) else {
            return false;
        };
        self.pixels[index] = color.0;
        true
    }

    /// Fills the portion of a rectangle that intersects the framebuffer.
    pub fn fill_rect(&mut self, rect: Rect, color: Gray8) {
        let Some(rect) = rect.intersection(Rect::from_size(self.size)) else {
            return;
        };
        let stride = self.stride_bytes();

        for y in rect.origin.y..rect.bottom() {
            let start = y as usize * stride + rect.origin.x as usize;
            let end = start + rect.size.width as usize;
            self.pixels[start..end].fill(color.0);
        }
    }

    /// Draws an inward rectangular stroke, clipped to the framebuffer.
    pub fn stroke_rect(&mut self, rect: Rect, width: u32, color: Gray8) {
        if width == 0 || rect.is_empty() {
            return;
        }

        let width = width.min(rect.size.width).min(rect.size.height);
        self.fill_rect(
            Rect::new(rect.origin.x, rect.origin.y, rect.size.width, width),
            color,
        );
        self.fill_rect(
            Rect::new(
                rect.origin.x,
                rect.bottom().saturating_sub(width),
                rect.size.width,
                width,
            ),
            color,
        );
        self.fill_rect(
            Rect::new(rect.origin.x, rect.origin.y, width, rect.size.height),
            color,
        );
        self.fill_rect(
            Rect::new(
                rect.right().saturating_sub(width),
                rect.origin.y,
                width,
                rect.size.height,
            ),
            color,
        );
    }

    fn index(&self, point: Point) -> Option<usize> {
        (point.x < self.size.width && point.y < self.size.height)
            .then(|| point.y as usize * self.stride_bytes() + point.x as usize)
    }
}

#[cfg(test)]
mod tests {
    use paper_display::{Point, Rect, Size};
    use proptest::prelude::*;

    use super::{Framebuffer, Gray8};

    #[test]
    fn clips_filled_rectangles() {
        let mut frame = Framebuffer::new(Size::new(4, 3), Gray8::WHITE).unwrap();
        frame.fill_rect(Rect::new(2, 1, 8, 8), Gray8::BLACK);

        assert_eq!(frame.get(Point::new(1, 1)), Some(Gray8::WHITE));
        assert_eq!(frame.get(Point::new(2, 1)), Some(Gray8::BLACK));
        assert_eq!(frame.get(Point::new(3, 2)), Some(Gray8::BLACK));
    }

    proptest! {
        #[test]
        fn arbitrary_drawing_never_changes_buffer_length(
            x in 0_u32..100,
            y in 0_u32..100,
            width in 0_u32..100,
            height in 0_u32..100,
            gray in any::<u8>(),
        ) {
            let mut frame = Framebuffer::new(Size::new(32, 24), Gray8::WHITE).unwrap();
            let len = frame.pixels().len();
            frame.fill_rect(Rect::new(x, y, width, height), Gray8(gray));
            prop_assert_eq!(frame.pixels().len(), len);
        }
    }
}
