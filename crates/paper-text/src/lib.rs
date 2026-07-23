//! Typography contracts and the host text-shaping backend.
//!
//! The public boundary emits raster coverage rectangles rather than backend
//! glyph identifiers. This keeps font fallback and glyph-cache identity inside
//! the engine that owns them.

use core::convert::Infallible;

use cosmic_text::{
    Align, Attrs, Buffer, Color, Ellipsize, EllipsizeHeightLimit, Family, FontSystem, Metrics,
    Shaping, SwashCache, Weight, Wrap,
};
use paper_display::Size;

/// A font-family selection.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum FontFamily {
    /// The configured sans-serif fallback family.
    SansSerif,
    /// The configured serif fallback family.
    Serif,
    /// The configured monospace fallback family.
    Monospace,
    /// A specific family name.
    Named(String),
}

impl FontFamily {
    /// Selects a specific named family.
    pub fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into())
    }

    /// Returns the named family, if this selection is name-specific.
    pub fn as_named_str(&self) -> Option<&str> {
        match self {
            Self::Named(name) => Some(name),
            Self::SansSerif | Self::Serif | Self::Monospace => None,
        }
    }
}

/// A CSS-like font weight.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FontWeight {
    /// Weight 100.
    Thin,
    /// Weight 300.
    Light,
    /// Weight 400.
    #[default]
    Regular,
    /// Weight 500.
    Medium,
    /// Weight 600.
    Semibold,
    /// Weight 700.
    Bold,
    /// Weight 900.
    Black,
}

/// Horizontal paragraph alignment.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextAlignment {
    /// Follow the paragraph's natural writing direction.
    #[default]
    Start,
    /// Center each line.
    Center,
    /// Align to the paragraph's trailing edge.
    End,
    /// Expand inter-word spacing to fill the line.
    Justified,
}

/// Behavior when text exceeds its paragraph bounds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextOverflow {
    /// Clip content outside the paragraph bounds.
    #[default]
    Clip,
    /// Ellipsize the final visible line.
    Ellipsis,
}

/// Invalid typography metrics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextStyleError {
    /// Font size and line height must both be finite positive values.
    InvalidMetrics,
}

/// Resolved physical typography used by a shaping backend.
///
/// ```
/// use paper_text::{FontFamily, FontWeight, TextAlignment, TextStyle};
///
/// let style = TextStyle::new(
///     FontFamily::SansSerif,
///     FontWeight::Bold,
///     32.0,
///     40.0,
///     TextAlignment::Start,
/// ).unwrap();
/// assert_eq!(style.size_px(), 32.0);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    family: FontFamily,
    weight: FontWeight,
    size_px: f32,
    line_height_px: f32,
    alignment: TextAlignment,
}

impl TextStyle {
    /// Creates a validated resolved text style.
    pub fn new(
        family: FontFamily,
        weight: FontWeight,
        size_px: f32,
        line_height_px: f32,
        alignment: TextAlignment,
    ) -> Result<Self, TextStyleError> {
        if !size_px.is_finite()
            || !line_height_px.is_finite()
            || size_px <= 0.0
            || line_height_px <= 0.0
        {
            return Err(TextStyleError::InvalidMetrics);
        }
        Ok(Self {
            family,
            weight,
            size_px,
            line_height_px,
            alignment,
        })
    }

    /// Returns the selected font family.
    pub const fn family(&self) -> &FontFamily {
        &self.family
    }

    /// Returns the selected font weight.
    pub const fn weight(&self) -> FontWeight {
        self.weight
    }

    /// Returns the resolved font size in physical pixels.
    pub const fn size_px(&self) -> f32 {
        self.size_px
    }

    /// Returns the resolved line height in physical pixels.
    pub const fn line_height_px(&self) -> f32 {
        self.line_height_px
    }

    /// Returns paragraph alignment.
    pub const fn alignment(&self) -> TextAlignment {
        self.alignment
    }
}

/// A borrowed paragraph ready for shaping in physical pixel bounds.
#[derive(Clone, Debug, PartialEq)]
pub struct Paragraph<'a> {
    /// UTF-8 paragraph content.
    pub text: &'a str,
    /// Resolved typography.
    pub style: &'a TextStyle,
    /// Maximum physical width and height.
    pub max_size: Size,
    /// Optional maximum number of visible lines.
    pub max_lines: Option<u32>,
    /// Overflow behavior.
    pub overflow: TextOverflow,
}

/// Measured output from shaping a paragraph.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextLayout {
    /// Tight visible size, clipped to the requested paragraph bounds.
    pub size: Size,
    /// Number of visible layout lines.
    pub line_count: u32,
}

/// One rasterized coverage rectangle relative to the paragraph origin.
///
/// Glyph masks normally produce one-pixel rectangles. Decorations can produce
/// wider rectangles without forcing the typography boundary to allocate a
/// framebuffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoverageRect {
    /// Signed horizontal offset; glyph overhang may be negative.
    pub x: i32,
    /// Signed vertical offset; glyph overhang may be negative.
    pub y: i32,
    /// Rectangle width.
    pub width: u32,
    /// Rectangle height.
    pub height: u32,
    /// Alpha-like ink coverage from transparent `0` through opaque `255`.
    pub coverage: u8,
}

/// Backend seam for shaping, wrapping, bidi resolution, and glyph rasterization.
pub trait TextEngine {
    /// Backend-specific error.
    type Error;

    /// Shapes and measures a paragraph without exposing backend glyph IDs.
    fn measure(&mut self, paragraph: &Paragraph<'_>) -> Result<TextLayout, Self::Error>;

    /// Shapes and rasterizes a paragraph into coverage rectangles.
    fn rasterize(
        &mut self,
        paragraph: &Paragraph<'_>,
        emit: &mut dyn FnMut(CoverageRect),
    ) -> Result<TextLayout, Self::Error>;
}

/// Host typography backend powered by `cosmic-text` and Swash.
///
/// Construct one engine and reuse it so font discovery, shaping, and raster
/// caches survive across renders.
#[derive(Debug)]
pub struct CosmicTextEngine {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl CosmicTextEngine {
    /// Loads host fonts and creates empty shaping/raster caches.
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
        }
    }

    fn prepare_buffer(&mut self, paragraph: &Paragraph<'_>) -> Buffer {
        let metrics = Metrics::new(paragraph.style.size_px, paragraph.style.line_height_px);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        let max_line_height = paragraph
            .max_lines
            .map(|lines| paragraph.style.line_height_px * pixel_count_as_f32(lines));
        let maximum_height = pixel_count_as_f32(paragraph.max_size.height);
        let height = max_line_height.map_or(maximum_height, |line_height| {
            line_height.min(maximum_height)
        });
        buffer.set_size(
            Some(pixel_count_as_f32(paragraph.max_size.width)),
            Some(height),
        );
        buffer.set_wrap(Wrap::WordOrGlyph);
        if paragraph.overflow == TextOverflow::Ellipsis {
            let limit = paragraph
                .max_lines
                .map_or(EllipsizeHeightLimit::Height(height), |lines| {
                    EllipsizeHeightLimit::Lines(usize::try_from(lines).unwrap_or(usize::MAX))
                });
            buffer.set_ellipsize(Ellipsize::End(limit));
        }

        let family = match &paragraph.style.family {
            FontFamily::SansSerif => Family::SansSerif,
            FontFamily::Serif => Family::Serif,
            FontFamily::Monospace => Family::Monospace,
            FontFamily::Named(name) => Family::Name(name),
        };
        let attrs = Attrs::new()
            .family(family)
            .weight(Weight(font_weight_value(paragraph.style.weight)));
        buffer.set_text(
            paragraph.text,
            &attrs,
            Shaping::Advanced,
            cosmic_alignment(paragraph.style.alignment),
        );
        buffer.shape_until_scroll(&mut self.font_system, false);
        buffer
    }
}

impl Default for CosmicTextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEngine for CosmicTextEngine {
    type Error = Infallible;

    fn measure(&mut self, paragraph: &Paragraph<'_>) -> Result<TextLayout, Self::Error> {
        let buffer = self.prepare_buffer(paragraph);
        Ok(measure_buffer(&buffer, paragraph.max_size))
    }

    fn rasterize(
        &mut self,
        paragraph: &Paragraph<'_>,
        emit: &mut dyn FnMut(CoverageRect),
    ) -> Result<TextLayout, Self::Error> {
        let mut buffer = self.prepare_buffer(paragraph);
        let layout = measure_buffer(&buffer, paragraph.max_size);
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            Color::rgb(0, 0, 0),
            |x, y, width, height, color| {
                emit(CoverageRect {
                    x,
                    y,
                    width,
                    height,
                    coverage: color.a(),
                });
            },
        );
        Ok(layout)
    }
}

const fn font_weight_value(weight: FontWeight) -> u16 {
    match weight {
        FontWeight::Thin => 100,
        FontWeight::Light => 300,
        FontWeight::Regular => 400,
        FontWeight::Medium => 500,
        FontWeight::Semibold => 600,
        FontWeight::Bold => 700,
        FontWeight::Black => 900,
    }
}

const fn cosmic_alignment(alignment: TextAlignment) -> Option<Align> {
    match alignment {
        TextAlignment::Start => None,
        TextAlignment::Center => Some(Align::Center),
        TextAlignment::End => Some(Align::End),
        TextAlignment::Justified => Some(Align::Justified),
    }
}

fn measure_buffer(buffer: &Buffer, max_size: Size) -> TextLayout {
    let mut width = 0.0_f32;
    let mut bottom = 0.0_f32;
    let mut line_count = 0_u32;
    for run in buffer.layout_runs() {
        width = width.max(run.line_w);
        bottom = bottom.max(run.line_top + run.line_height);
        line_count = line_count.saturating_add(1);
    }
    TextLayout {
        size: Size::new(
            ceil_to_u32(width).min(max_size.width),
            ceil_to_u32(bottom).min(max_size.height),
        ),
        line_count,
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn ceil_to_u32(value: f32) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else if value >= u32::MAX as f32 {
        u32::MAX
    } else {
        value.ceil() as u32
    }
}

#[allow(clippy::cast_precision_loss)]
fn pixel_count_as_f32(value: u32) -> f32 {
    value as f32
}

#[cfg(test)]
mod tests {
    use paper_display::Size;

    use super::{
        CosmicTextEngine, FontFamily, FontWeight, Paragraph, TextAlignment, TextEngine,
        TextOverflow, TextStyle, TextStyleError,
    };

    fn style() -> TextStyle {
        TextStyle::new(
            FontFamily::SansSerif,
            FontWeight::Regular,
            24.0,
            30.0,
            TextAlignment::Start,
        )
        .unwrap()
    }

    #[test]
    fn text_metrics_must_be_finite_and_positive() {
        assert_eq!(
            TextStyle::new(
                FontFamily::SansSerif,
                FontWeight::Regular,
                f32::NAN,
                20.0,
                TextAlignment::Start,
            ),
            Err(TextStyleError::InvalidMetrics)
        );
        assert_eq!(
            TextStyle::new(
                FontFamily::SansSerif,
                FontWeight::Regular,
                10.0,
                0.0,
                TextAlignment::Start,
            ),
            Err(TextStyleError::InvalidMetrics)
        );
    }

    #[test]
    fn cosmic_backend_shapes_and_rasterizes_with_font_identity_kept_internal() {
        let style = style();
        let paragraph = Paragraph {
            text: "PaperOS typography",
            style: &style,
            max_size: Size::new(400, 100),
            max_lines: Some(2),
            overflow: TextOverflow::Ellipsis,
        };
        let mut engine = CosmicTextEngine::new();
        let measured = engine.measure(&paragraph).unwrap();
        let mut coverage = Vec::new();
        let rasterized = engine
            .rasterize(&paragraph, &mut |rect| coverage.push(rect))
            .unwrap();

        assert_eq!(rasterized, measured);
        assert!(measured.size.width > 0);
        assert!(measured.size.height > 0);
        assert!(!coverage.is_empty());
        assert!(coverage.iter().any(|rect| rect.coverage > 0));
    }
}
