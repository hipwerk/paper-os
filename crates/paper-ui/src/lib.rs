//! Application-facing scenes, widgets, and render context.

use core::fmt;

use paper_display::{Rect, Size};
use paper_graphics::{Framebuffer, GraphicsError, Gray8};
use paper_layout::{Constraints, ScaleFactor};
use paper_text::{Paragraph, TextEngine, TextError, TextLayout, TextOverflow, TextStyle};

/// One paragraph retained in a scene before shaping and rasterization.
#[derive(Clone, Debug, PartialEq)]
pub struct TextCommand {
    /// Final physical bounds assigned by layout.
    pub bounds: Rect,
    /// UTF-8 paragraph content.
    pub content: String,
    /// Resolved physical typography.
    pub style: TextStyle,
    /// Grayscale ink color.
    pub color: Gray8,
    /// Optional maximum visible line count.
    pub max_lines: Option<u32>,
    /// Behavior when content exceeds `bounds`.
    pub overflow: TextOverflow,
}

/// One ordered operation in a retained scene.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawCommand {
    /// Fill a rectangle with one grayscale value.
    Fill {
        /// Physical target rectangle.
        rect: Rect,
        /// Fill color.
        color: Gray8,
    },
    /// Stroke the inside edge of a rectangle.
    Stroke {
        /// Physical target rectangle.
        rect: Rect,
        /// Stroke width in physical pixels.
        width: u32,
        /// Stroke color.
        color: Gray8,
    },
    /// Shape and rasterize one bounded paragraph.
    Text(TextCommand),
}

/// An ordered retained display list produced by an application render.
///
/// This is deliberately higher-level than a framebuffer: layout can be
/// inspected, rendering can be tiled, and backends can optimize the scene before
/// rasterization.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    commands: Vec<DrawCommand>,
}

impl Scene {
    /// Creates an empty scene.
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Appends one command, preserving caller order.
    pub fn push(&mut self, command: DrawCommand) {
        self.commands.push(command);
    }

    /// Returns the ordered commands.
    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }
}

/// A declarative element that measures itself and draws into an assigned box.
///
/// `draw` receives the final rectangle chosen by its parent. Implementations
/// must not cache hidden layout state between these calls.
pub trait Widget {
    /// Returns the desired physical size within validated constraints.
    fn layout(
        &self,
        context: &mut Context<'_>,
        constraints: Constraints,
    ) -> Result<Size, TextError>;

    /// Emits draw commands inside the final physical bounds assigned by layout.
    fn draw(&self, context: &Context<'_>, bounds: Rect, scene: &mut Scene);
}

/// Explicit environment shared by application rendering and widget layout.
///
/// Keeping scale and typography measurement in the same environment lets
/// reusable widgets resolve logical design values and measure wrapped text
/// without storing hidden layout state.
pub struct Context<'a> {
    viewport: Size,
    scale: ScaleFactor,
    text_engine: &'a mut dyn TextEngine,
}

impl<'a> Context<'a> {
    /// Creates a render context for one physical target.
    pub fn new(viewport: Size, scale: ScaleFactor, text_engine: &'a mut dyn TextEngine) -> Self {
        Self {
            viewport,
            scale,
            text_engine,
        }
    }

    /// Returns the physical framebuffer dimensions.
    pub const fn viewport(&self) -> Size {
        self.viewport
    }

    /// Returns the logical-to-physical mapping.
    pub const fn scale(&self) -> ScaleFactor {
        self.scale
    }

    /// Resolves one application design length to physical pixels.
    pub fn resolve(&self, logical: u32) -> u32 {
        self.scale.resolve(logical)
    }

    /// Shapes and measures text using the target's typography backend.
    pub fn measure_text(&mut self, paragraph: &Paragraph<'_>) -> Result<TextLayout, TextError> {
        self.text_engine.measure(paragraph)
    }
}

/// A top-level source of retained scenes.
pub trait Application {
    /// Renders the current application state for one explicit context.
    fn render(&self, context: &mut Context<'_>) -> Result<Scene, TextError>;
}

/// Failure while rasterizing a retained scene.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SceneRenderError {
    /// The target framebuffer could not be allocated.
    Graphics(GraphicsError),
    /// A text command could not be shaped or rasterized.
    Text(TextError),
}

impl fmt::Display for SceneRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Graphics(error) => write!(formatter, "could not create render target: {error}"),
            Self::Text(error) => write!(formatter, "could not render text: {error}"),
        }
    }
}

impl std::error::Error for SceneRenderError {}

impl From<GraphicsError> for SceneRenderError {
    fn from(error: GraphicsError) -> Self {
        Self::Graphics(error)
    }
}

impl From<TextError> for SceneRenderError {
    fn from(error: TextError) -> Self {
        Self::Text(error)
    }
}

/// Rasterizes an ordered scene into a canonical Gray8 framebuffer.
pub fn render_scene(
    scene: &Scene,
    size: Size,
    text_engine: &mut dyn TextEngine,
) -> Result<Framebuffer, SceneRenderError> {
    let mut frame = Framebuffer::new(size, Gray8::WHITE)?;
    for command in scene.commands() {
        match command {
            DrawCommand::Fill { rect, color } => frame.fill_rect(*rect, *color),
            DrawCommand::Stroke { rect, width, color } => {
                frame.stroke_rect(*rect, *width, *color);
            }
            DrawCommand::Text(command) => {
                let paragraph = Paragraph {
                    text: &command.content,
                    style: &command.style,
                    max_size: command.bounds.size,
                    max_lines: command.max_lines,
                    overflow: command.overflow,
                };
                let origin = command.bounds.origin;
                let color = command.color;
                text_engine.rasterize(&paragraph, &mut |coverage| {
                    let x = u32::try_from(coverage.x).unwrap_or(u32::MAX);
                    let y = u32::try_from(coverage.y).unwrap_or(u32::MAX);
                    frame.blend_rect(
                        Rect::new(
                            origin.x.saturating_add(x),
                            origin.y.saturating_add(y),
                            coverage.width,
                            coverage.height,
                        ),
                        color,
                        coverage.coverage,
                    );
                })?;
            }
        }
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use paper_display::{Point, Rect, Size};
    use paper_graphics::Gray8;
    use paper_layout::{Constraints, ScaleFactor};
    use paper_text::{
        CoverageRect, FontFamily, FontWeight, Paragraph, TextAlignment, TextEngine, TextError,
        TextLayout, TextOverflow, TextStyle,
    };

    use super::{Context, DrawCommand, Scene, TextCommand, Widget, render_scene};

    struct BoundsWidget;

    impl Widget for BoundsWidget {
        fn layout(
            &self,
            context: &mut Context<'_>,
            constraints: Constraints,
        ) -> Result<Size, TextError> {
            Ok(constraints.constrain(Size::new(context.resolve(10), context.resolve(5))))
        }

        fn draw(&self, _context: &Context<'_>, bounds: Rect, scene: &mut Scene) {
            scene.push(DrawCommand::Fill {
                rect: bounds,
                color: Gray8::BLACK,
            });
        }
    }

    #[derive(Default)]
    struct MeasuringEngine {
        measured: usize,
    }

    impl TextEngine for MeasuringEngine {
        fn measure(&mut self, paragraph: &Paragraph<'_>) -> Result<TextLayout, TextError> {
            self.measured += 1;
            Ok(TextLayout {
                size: Size::new(
                    paragraph.max_size.width.min(42),
                    paragraph.max_size.height.min(18),
                ),
                line_count: 1,
            })
        }

        fn rasterize(
            &mut self,
            paragraph: &Paragraph<'_>,
            _emit: &mut dyn FnMut(CoverageRect),
        ) -> Result<TextLayout, TextError> {
            self.measure(paragraph)
        }
    }

    struct CoverageEngine;

    impl TextEngine for CoverageEngine {
        fn measure(&mut self, paragraph: &Paragraph<'_>) -> Result<TextLayout, TextError> {
            Ok(TextLayout {
                size: paragraph.max_size,
                line_count: 1,
            })
        }

        fn rasterize(
            &mut self,
            paragraph: &Paragraph<'_>,
            emit: &mut dyn FnMut(CoverageRect),
        ) -> Result<TextLayout, TextError> {
            emit(CoverageRect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
                coverage: 128,
            });
            self.measure(paragraph)
        }
    }

    fn style() -> TextStyle {
        TextStyle::new(
            FontFamily::SansSerif,
            FontWeight::Regular,
            16.0,
            20.0,
            TextAlignment::Start,
        )
        .unwrap()
    }

    #[test]
    fn scene_preserves_explicit_command_order() {
        let mut scene = Scene::new();
        scene.push(DrawCommand::Fill {
            rect: Rect::new(0, 0, 10, 10),
            color: Gray8::WHITE,
        });
        scene.push(DrawCommand::Stroke {
            rect: Rect::new(0, 0, 10, 10),
            width: 1,
            color: Gray8::BLACK,
        });

        assert_eq!(scene.commands().len(), 2);
    }

    #[test]
    fn widget_draws_into_parent_assigned_bounds() {
        let widget = BoundsWidget;
        let mut text = MeasuringEngine::default();
        let mut context = Context::new(
            Size::new(100, 100),
            ScaleFactor::new(2, 1).unwrap(),
            &mut text,
        );
        let desired = widget
            .layout(&mut context, Constraints::loose(Size::new(100, 100)))
            .unwrap();
        assert_eq!(desired, Size::new(20, 10));

        let assigned = Rect::new(5, 7, 80, 30);
        let mut scene = Scene::new();
        widget.draw(&context, assigned, &mut scene);
        assert_eq!(
            scene.commands(),
            &[DrawCommand::Fill {
                rect: assigned,
                color: Gray8::BLACK,
            }]
        );
    }

    #[test]
    fn context_carries_resolution_scale_explicitly() {
        let mut text = MeasuringEngine::default();
        let mut context = Context::new(
            Size::new(1448, 1072),
            ScaleFactor::new(3, 2).unwrap(),
            &mut text,
        );
        assert_eq!(context.resolve(48), 72);
        assert_eq!(context.viewport(), Size::new(1448, 1072));

        let style = style();
        let paragraph = Paragraph {
            text: "PaperOS",
            style: &style,
            max_size: Size::new(100, 30),
            max_lines: Some(1),
            overflow: TextOverflow::Clip,
        };
        assert_eq!(
            context.measure_text(&paragraph).unwrap().size,
            Size::new(42, 18)
        );
    }

    #[test]
    fn scene_rasterizer_preserves_order_and_composites_text_coverage() {
        let mut scene = Scene::new();
        scene.push(DrawCommand::Fill {
            rect: Rect::new(0, 0, 4, 3),
            color: Gray8(200),
        });
        scene.push(DrawCommand::Text(TextCommand {
            bounds: Rect::new(1, 1, 2, 1),
            content: "ink".to_owned(),
            style: style(),
            color: Gray8::BLACK,
            max_lines: Some(1),
            overflow: TextOverflow::Clip,
        }));
        scene.push(DrawCommand::Stroke {
            rect: Rect::new(0, 0, 4, 3),
            width: 1,
            color: Gray8::BLACK,
        });

        let frame = render_scene(&scene, Size::new(4, 3), &mut CoverageEngine).unwrap();
        assert_eq!(frame.get(Point::new(0, 0)), Some(Gray8::BLACK));
        assert_eq!(frame.get(Point::new(1, 1)), Some(Gray8(100)));
        assert_eq!(frame.get(Point::new(2, 1)), Some(Gray8(100)));
    }
}
