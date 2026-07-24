//! Deterministic multilingual typography specimen shared by preview and lab.

use paper_display::{Rect, Size};
use paper_graphics::{Framebuffer, Gray8};
use paper_layout::ScaleFactor;
use paper_text::{
    CosmicTextEngine, FontFamily, FontWeight, TextAlignment, TextError, TextOverflow, TextStyle,
};
use paper_ui::{
    Application, Context, DrawCommand, Scene, SceneRenderError, TextCommand, render_scene,
};

/// Logical portrait viewport used by the 6-inch reference fixture.
pub const SPECIMEN_SIZE: Size = Size::new(1072, 1448);

const SOURCE_SERIF_FAMILY: &str = "Source Serif 4";
const NOTO_NASKH_FAMILY: &str = "Noto Naskh Arabic";
const SOURCE_SERIF: &[u8] =
    include_bytes!("../../../assets/fonts/source-serif-4/SourceSerif4-Variable.ttf");
const NOTO_NASKH_ARABIC: &[u8] =
    include_bytes!("../../../assets/fonts/noto-naskh-arabic/NotoNaskhArabic-Variable.ttf");

/// The reference page used to validate PaperOS typography.
pub struct TypographySpecimen;

impl Application for TypographySpecimen {
    fn render(&self, context: &mut Context<'_>) -> Result<Scene, TextError> {
        let mut scene = Scene::new();
        scene.push(DrawCommand::Fill {
            rect: Rect::from_size(context.viewport()),
            color: Gray8::WHITE,
        });
        add_header(&mut scene)?;
        add_body(&mut scene)?;
        add_language_proof(&mut scene)?;
        add_tonal_footer(&mut scene)?;
        Ok(scene)
    }
}

fn add_header(scene: &mut Scene) -> Result<(), TextError> {
    scene.push(text(
        Rect::new(88, 72, 896, 42),
        "PAPEROS · TYPOGRAPHY PROOF",
        style(SOURCE_SERIF_FAMILY, FontWeight::Semibold, 26.0, 34.0)?,
        Gray8(72),
        Some(1),
        TextOverflow::Clip,
    ));
    scene.push(text(
        Rect::new(84, 138, 904, 92),
        "A page worth keeping.",
        style(SOURCE_SERIF_FAMILY, FontWeight::Bold, 70.0, 84.0)?,
        Gray8::BLACK,
        Some(1),
        TextOverflow::Clip,
    ));
    scene.push(DrawCommand::Fill {
        rect: Rect::new(88, 278, 896, 3),
        color: Gray8(32),
    });
    Ok(())
}

fn add_body(scene: &mut Scene) -> Result<(), TextError> {
    scene.push(text(
        Rect::new(88, 326, 610, 288),
        "Reflective displays reward restraint. PaperOS treats every refresh \
         as a deliberate act and every pixel as ink on a permanent page. \
         Typography, rhythm, and whitespace come before motion.",
        style(SOURCE_SERIF_FAMILY, FontWeight::Regular, 35.0, 48.0)?,
        Gray8(24),
        Some(6),
        TextOverflow::Clip,
    ));
    scene.push(text(
        Rect::new(742, 334, 242, 206),
        "Aa\n24 / 07\n1448 × 1072",
        style(SOURCE_SERIF_FAMILY, FontWeight::Semibold, 31.0, 52.0)?,
        Gray8(92),
        Some(4),
        TextOverflow::Clip,
    ));
    Ok(())
}

fn add_language_proof(scene: &mut Scene) -> Result<(), TextError> {
    scene.push(DrawCommand::Stroke {
        rect: Rect::new(88, 642, 896, 172),
        width: 2,
        color: Gray8(112),
    });
    scene.push(text(
        Rect::new(116, 672, 840, 104),
        "Efficient affinity — office, fjord, Straße, déjà vu.\n\
         «La clarté naît d’une mise en page attentive.»",
        style(SOURCE_SERIF_FAMILY, FontWeight::Regular, 31.0, 43.0)?,
        Gray8(32),
        Some(2),
        TextOverflow::Clip,
    ));
    scene.push(text(
        Rect::new(88, 862, 896, 114),
        "الوضوح والبساطة يصنعان صفحة تدوم",
        style(NOTO_NASKH_FAMILY, FontWeight::Regular, 48.0, 68.0)?,
        Gray8(20),
        Some(1),
        TextOverflow::Clip,
    ));
    scene.push(text(
        Rect::new(88, 950, 896, 42),
        "Clarity and simplicity make a page endure.",
        style(SOURCE_SERIF_FAMILY, FontWeight::Regular, 25.0, 34.0)?,
        Gray8(112),
        Some(1),
        TextOverflow::Clip,
    ));
    Ok(())
}

fn add_tonal_footer(scene: &mut Scene) -> Result<(), TextError> {
    for index in 0_u32..8 {
        let gray = u8::try_from(index * 32).unwrap_or(u8::MAX);
        scene.push(DrawCommand::Fill {
            rect: Rect::new(88 + index * 112, 1054, 112, 74),
            color: Gray8(gray),
        });
    }
    scene.push(text(
        Rect::new(88, 1150, 896, 86),
        "INK  ·  SPACE  ·  SILENCE",
        style(SOURCE_SERIF_FAMILY, FontWeight::Bold, 38.0, 50.0)?,
        Gray8(48),
        Some(1),
        TextOverflow::Clip,
    ));
    scene.push(DrawCommand::Fill {
        rect: Rect::new(88, 1260, 896, 1),
        color: Gray8(128),
    });
    scene.push(text(
        Rect::new(88, 1292, 896, 72),
        "Source Serif 4 + Noto Naskh Arabic · Gray8 master · Gray4 glass",
        style(SOURCE_SERIF_FAMILY, FontWeight::Regular, 23.0, 32.0)?,
        Gray8(104),
        Some(2),
        TextOverflow::Clip,
    ));
    Ok(())
}

/// Creates a font engine that cannot fall back to host-installed fonts.
pub fn specimen_text_engine() -> CosmicTextEngine {
    CosmicTextEngine::new_with_font_data([SOURCE_SERIF.to_vec(), NOTO_NASKH_ARABIC.to_vec()])
}

/// Renders the complete specimen to canonical Gray8 pixels.
pub fn render_specimen() -> Result<Framebuffer, SceneRenderError> {
    let mut engine = specimen_text_engine();
    let scene = {
        let mut context = Context::new(
            SPECIMEN_SIZE,
            ScaleFactor::new(1, 1).expect("unit scale is valid"),
            &mut engine,
        );
        TypographySpecimen
            .render(&mut context)
            .map_err(SceneRenderError::Text)?
    };
    render_scene(&scene, SPECIMEN_SIZE, &mut engine)
}

fn style(
    family: &str,
    weight: FontWeight,
    size: f32,
    line_height: f32,
) -> Result<TextStyle, TextError> {
    TextStyle::new(
        FontFamily::named(family),
        weight,
        size,
        line_height,
        TextAlignment::Start,
    )
    .map_err(|_| TextError::backend("specimen contains invalid typography metrics"))
}

fn text(
    bounds: Rect,
    content: &str,
    style: TextStyle,
    color: Gray8,
    max_lines: Option<u32>,
    overflow: TextOverflow,
) -> DrawCommand {
    DrawCommand::Text(TextCommand {
        bounds,
        content: content.to_owned(),
        style,
        color,
        max_lines,
        overflow,
    })
}

#[cfg(test)]
mod tests {
    use paper_display::{Point, Size};
    use paper_graphics::Gray8;

    use super::{SPECIMEN_SIZE, render_specimen};

    #[test]
    fn specimen_is_deterministic_and_uses_the_portrait_viewport() {
        let first = render_specimen().unwrap();
        let second = render_specimen().unwrap();

        assert_eq!(first, second);
        assert_eq!(first.size(), Size::new(1072, 1448));
        assert_eq!(first.size(), SPECIMEN_SIZE);
        assert_eq!(first.get(Point::new(0, 0)), Some(Gray8::WHITE));
        assert!(first.pixels().contains(&0));
        assert!(first.pixels().iter().any(|pixel| (1..=254).contains(pixel)));
    }
}
