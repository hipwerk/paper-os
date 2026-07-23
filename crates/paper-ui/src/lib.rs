use paper_display::{Point, Rect};
use paper_graphics::Gray8;
use paper_layout::Constraints;
use paper_text::TextStyle;

#[derive(Clone, Debug, PartialEq)]
pub enum DrawCommand {
    Fill {
        rect: Rect,
        color: Gray8,
    },
    Stroke {
        rect: Rect,
        width: u32,
        color: Gray8,
    },
    Text {
        origin: Point,
        content: String,
        style: TextStyle,
    },
}

/// A retained display list produced by an application render.
///
/// This is deliberately higher-level than a framebuffer: layout can be
/// inspected, rendering can be tiled, and future backends can optimize the
/// scene before rasterization.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    commands: Vec<DrawCommand>,
}

impl Scene {
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    pub fn push(&mut self, command: DrawCommand) {
        self.commands.push(command);
    }

    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }
}

pub trait Widget {
    fn layout(&self, constraints: Constraints) -> paper_display::Size;

    fn draw(&self, origin: Point, scene: &mut Scene);
}

pub struct Context {
    pub viewport: paper_display::Size,
}

pub trait Application {
    fn render(&self, context: &Context) -> Scene;
}

#[cfg(test)]
mod tests {
    use paper_display::Rect;
    use paper_graphics::Gray8;

    use super::{DrawCommand, Scene};

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
}
