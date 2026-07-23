use paper_display::Size;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FontFamily(String);

impl FontFamily {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FontWeight {
    Thin,
    Light,
    #[default]
    Regular,
    Medium,
    Semibold,
    Bold,
    Black,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextAlignment {
    #[default]
    Start,
    Center,
    End,
    Justified,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub family: FontFamily,
    pub weight: FontWeight,
    pub size_px: f32,
    pub line_height_px: f32,
    pub alignment: TextAlignment,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Paragraph<'a> {
    pub text: &'a str,
    pub style: &'a TextStyle,
    pub max_size: Size,
    pub max_lines: Option<u32>,
    pub overflow: TextOverflow,
}

/// A positioned glyph emitted by a shaping backend.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub advance: f32,
}

/// Backend seam for shaping, wrapping, and bidi resolution.
///
/// The first implementation is expected to wrap `cosmic-text`; this trait keeps
/// that dependency and its cache lifecycle out of application and UI APIs.
pub trait TextEngine {
    type Error;

    fn measure(&mut self, paragraph: &Paragraph<'_>) -> Result<Size, Self::Error>;

    fn shape(
        &mut self,
        paragraph: &Paragraph<'_>,
        emit: &mut dyn FnMut(Glyph),
    ) -> Result<Size, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::FontFamily;

    #[test]
    fn font_family_preserves_human_readable_name() {
        assert_eq!(FontFamily::new("Inter").as_str(), "Inter");
    }
}
