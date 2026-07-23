use paper_display::Size;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AssetId(String);

impl AssetId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Bmp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageMetadata {
    pub id: AssetId,
    pub size: Size,
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
