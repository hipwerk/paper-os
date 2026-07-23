//! Application-facing scenes, widgets, and render context.

use paper_display::{Rect, Size};
use paper_graphics::Gray8;
use paper_layout::{Constraints, ScaleFactor};
use paper_text::{TextOverflow, TextStyle};

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
    fn layout(&self, constraints: Constraints) -> Size;

    /// Emits draw commands inside the final physical bounds assigned by layout.
    fn draw(&self, bounds: Rect, scene: &mut Scene);
}

/// Explicit inputs available while rendering an application.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Context {
    /// Physical framebuffer dimensions.
    pub viewport: Size,
    /// Mapping from application design units to physical pixels.
    pub scale: ScaleFactor,
}

/// A top-level source of retained scenes.
pub trait Application {
    /// Renders the current application state for one explicit context.
    fn render(&self, context: &Context) -> Scene;
}

#[cfg(test)]
mod tests {
    use paper_display::{Rect, Size};
    use paper_graphics::Gray8;
    use paper_layout::{Constraints, ScaleFactor};

    use super::{DrawCommand, Scene, Widget};

    struct BoundsWidget;

    impl Widget for BoundsWidget {
        fn layout(&self, constraints: Constraints) -> Size {
            constraints.constrain(Size::new(20, 10))
        }

        fn draw(&self, bounds: Rect, scene: &mut Scene) {
            scene.push(DrawCommand::Fill {
                rect: bounds,
                color: Gray8::BLACK,
            });
        }
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
        let desired = widget.layout(Constraints::loose(Size::new(100, 100)));
        assert_eq!(desired, Size::new(20, 10));

        let assigned = Rect::new(5, 7, 80, 30);
        let mut scene = Scene::new();
        widget.draw(assigned, &mut scene);
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
        let context = super::Context {
            viewport: Size::new(1448, 1072),
            scale: ScaleFactor::new(3, 2).unwrap(),
        };
        assert_eq!(context.scale.resolve(48), 72);
    }
}
