//! Stable asset identifiers and image metadata.

use paper_display::Size;

/// A stable application-facing asset identifier.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AssetId(String);

impl AssetId {
    /// Creates an identifier from a human-readable stable string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Encoded raster image format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageFormat {
    /// Portable Network Graphics.
    Png,
    /// Joint Photographic Experts Group.
    Jpeg,
    /// Windows bitmap.
    Bmp,
}

/// Identity and decoded dimensions of one image asset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageMetadata {
    /// Stable asset identifier.
    pub id: AssetId,
    /// Native encoded pixel dimensions.
    pub size: Size,
    /// Encoded file format.
    pub format: ImageFormat,
}

#[cfg(test)]
mod tests {
    use super::AssetId;

    #[test]
    fn asset_ids_are_stable_strings() {
        let id = AssetId::new("weather/icon/sun");
        assert_eq!(id.as_str(), "weather/icon/sun");
    }
}
