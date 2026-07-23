use paper_display::{Rect, Size};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Insets {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

impl Insets {
    pub const fn all(value: u32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub const fn horizontal(self) -> u32 {
        self.left.saturating_add(self.right)
    }

    pub const fn vertical(self) -> u32 {
        self.top.saturating_add(self.bottom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Constraints {
    pub min: Size,
    pub max: Size,
}

impl Constraints {
    pub const fn tight(size: Size) -> Self {
        Self {
            min: size,
            max: size,
        }
    }

    pub fn constrain(self, size: Size) -> Size {
        Size::new(
            size.width.clamp(self.min.width, self.max.width),
            size.height.clamp(self.min.height, self.max.height),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrossAlignment {
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinearItem {
    pub natural_size: Size,
    pub grow: u16,
}

/// Deterministically lays out a Row or Column into caller-provided storage.
///
/// The function returns how many items were written. This allocation-free core
/// is usable by both a desktop renderer and a future constrained target.
pub fn linear_layout(
    axis: Axis,
    bounds: Rect,
    gap: u32,
    alignment: CrossAlignment,
    items: &[LinearItem],
    output: &mut [Rect],
) -> usize {
    let count = items.len().min(output.len());
    let items = &items[..count];
    if items.is_empty() {
        return 0;
    }

    let available_main = match axis {
        Axis::Horizontal => bounds.size.width,
        Axis::Vertical => bounds.size.height,
    };
    let gap_count = u32::try_from(count.saturating_sub(1)).unwrap_or(u32::MAX);
    let gaps = gap.saturating_mul(gap_count);
    let natural_main: u32 = items
        .iter()
        .map(|item| match axis {
            Axis::Horizontal => item.natural_size.width,
            Axis::Vertical => item.natural_size.height,
        })
        .fold(0, u32::saturating_add);
    let extra = available_main.saturating_sub(natural_main.saturating_add(gaps));
    let total_grow: u32 = items.iter().map(|item| u32::from(item.grow)).sum();
    let mut cursor = match axis {
        Axis::Horizontal => bounds.origin.x,
        Axis::Vertical => bounds.origin.y,
    };

    for (index, item) in items.iter().enumerate() {
        let natural = match axis {
            Axis::Horizontal => item.natural_size.width,
            Axis::Vertical => item.natural_size.height,
        };
        let share = extra
            .saturating_mul(u32::from(item.grow))
            .checked_div(total_grow)
            .unwrap_or(0);
        let main = natural.saturating_add(share);
        output[index] = place_item(axis, bounds, cursor, main, item.natural_size, alignment);
        cursor = cursor.saturating_add(main).saturating_add(gap);
    }
    count
}

fn place_item(
    axis: Axis,
    bounds: Rect,
    cursor: u32,
    main: u32,
    natural: Size,
    alignment: CrossAlignment,
) -> Rect {
    let (cross_origin, cross_size) = match axis {
        Axis::Horizontal => align_cross(
            bounds.origin.y,
            bounds.size.height,
            natural.height,
            alignment,
        ),
        Axis::Vertical => align_cross(bounds.origin.x, bounds.size.width, natural.width, alignment),
    };

    match axis {
        Axis::Horizontal => Rect::new(cursor, cross_origin, main, cross_size),
        Axis::Vertical => Rect::new(cross_origin, cursor, cross_size, main),
    }
}

fn align_cross(origin: u32, available: u32, natural: u32, alignment: CrossAlignment) -> (u32, u32) {
    let size = if alignment == CrossAlignment::Stretch {
        available
    } else {
        natural.min(available)
    };
    let free = available.saturating_sub(size);
    let offset = match alignment {
        CrossAlignment::Start | CrossAlignment::Stretch => 0,
        CrossAlignment::Center => free / 2,
        CrossAlignment::End => free,
    };
    (origin.saturating_add(offset), size)
}

#[cfg(test)]
mod tests {
    use paper_display::{Rect, Size};

    use super::{Axis, CrossAlignment, LinearItem, linear_layout};

    #[test]
    fn column_distributes_extra_space_to_growing_items() {
        let items = [
            LinearItem {
                natural_size: Size::new(20, 10),
                grow: 0,
            },
            LinearItem {
                natural_size: Size::new(30, 10),
                grow: 1,
            },
        ];
        let mut output = [Rect::default(); 2];

        linear_layout(
            Axis::Vertical,
            Rect::new(0, 0, 100, 50),
            5,
            CrossAlignment::Center,
            &items,
            &mut output,
        );

        assert_eq!(output[0], Rect::new(40, 0, 20, 10));
        assert_eq!(output[1], Rect::new(35, 15, 30, 35));
    }
}
