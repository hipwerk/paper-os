//! Deterministic, allocation-free layout primitives.
//!
//! Application-facing design values are resolved through [`ScaleFactor`] before
//! this crate places rectangles in physical framebuffer pixels.

#![no_std]

use paper_display::{Rect, Size};

/// Insets in resolved physical pixels.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Insets {
    /// Top inset.
    pub top: u32,
    /// Right inset.
    pub right: u32,
    /// Bottom inset.
    pub bottom: u32,
    /// Left inset.
    pub left: u32,
}

impl Insets {
    /// Creates equal insets on every side.
    pub const fn all(value: u32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    /// Returns the combined left and right inset.
    pub const fn horizontal(self) -> u32 {
        self.left.saturating_add(self.right)
    }

    /// Returns the combined top and bottom inset.
    pub const fn vertical(self) -> u32 {
        self.top.saturating_add(self.bottom)
    }
}

/// A size expressed in application design units.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LogicalSize {
    /// Logical width.
    pub width: u32,
    /// Logical height.
    pub height: u32,
}

impl LogicalSize {
    /// Creates a logical size.
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

/// A deterministic rational mapping from design units to physical pixels.
///
/// `ScaleFactor::new(3, 2)` maps two logical units to three physical pixels.
/// Resolution uses round-to-nearest with ties rounded upward.
///
/// ```
/// use paper_layout::{LogicalSize, ScaleFactor};
///
/// let scale = ScaleFactor::new(3, 2).unwrap();
/// assert_eq!(scale.resolve_size(LogicalSize::new(20, 10)).width, 30);
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScaleFactor {
    numerator: u32,
    denominator: u32,
}

impl ScaleFactor {
    /// A one-to-one mapping between logical and physical pixels.
    pub const ONE: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    /// Creates a non-zero rational scale.
    pub const fn new(numerator: u32, denominator: u32) -> Option<Self> {
        if numerator == 0 || denominator == 0 {
            None
        } else {
            Some(Self {
                numerator,
                denominator,
            })
        }
    }

    /// Returns the scale numerator.
    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    /// Returns the scale denominator.
    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    /// Resolves one logical length to physical pixels.
    pub fn resolve(self, logical: u32) -> u32 {
        let scaled = u64::from(logical).saturating_mul(u64::from(self.numerator));
        let rounded =
            scaled.saturating_add(u64::from(self.denominator) / 2) / u64::from(self.denominator);
        u32::try_from(rounded).unwrap_or(u32::MAX)
    }

    /// Resolves a logical size to physical pixels.
    pub fn resolve_size(self, logical: LogicalSize) -> Size {
        Size::new(self.resolve(logical.width), self.resolve(logical.height))
    }
}

/// An invalid layout input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutError {
    /// At least one minimum dimension is greater than its maximum.
    InvalidConstraints {
        /// Requested minimum dimensions.
        min: Size,
        /// Requested maximum dimensions.
        max: Size,
    },
}

/// Minimum and maximum physical dimensions accepted by a layout node.
///
/// ```
/// use paper_display::Size;
/// use paper_layout::Constraints;
///
/// let constraints = Constraints::new(Size::new(10, 10), Size::new(100, 50)).unwrap();
/// assert_eq!(constraints.constrain(Size::new(200, 5)), Size::new(100, 10));
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Constraints {
    min: Size,
    max: Size,
}

impl Constraints {
    /// Creates validated constraints.
    pub const fn new(min: Size, max: Size) -> Result<Self, LayoutError> {
        if min.width > max.width || min.height > max.height {
            Err(LayoutError::InvalidConstraints { min, max })
        } else {
            Ok(Self { min, max })
        }
    }

    /// Creates constraints requiring exactly `size`.
    pub const fn tight(size: Size) -> Self {
        Self {
            min: size,
            max: size,
        }
    }

    /// Creates constraints from zero through `max`.
    pub const fn loose(max: Size) -> Self {
        Self {
            min: Size::new(0, 0),
            max,
        }
    }

    /// Returns the minimum size.
    pub const fn min(self) -> Size {
        self.min
    }

    /// Returns the maximum size.
    pub const fn max(self) -> Size {
        self.max
    }

    /// Clamps a size to these validated constraints.
    pub fn constrain(self, size: Size) -> Size {
        Size::new(
            size.width.clamp(self.min.width, self.max.width),
            size.height.clamp(self.min.height, self.max.height),
        )
    }
}

/// The main direction of a linear layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Axis {
    /// Place items from left to right.
    Horizontal,
    /// Place items from top to bottom.
    Vertical,
}

/// Placement on the axis perpendicular to the main direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrossAlignment {
    /// Align with the leading cross-axis edge.
    Start,
    /// Center within the available cross-axis space.
    Center,
    /// Align with the trailing cross-axis edge.
    End,
    /// Fill the available cross-axis space.
    Stretch,
}

/// One child in a linear layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinearItem {
    /// Preferred resolved size before growth or shrinkage.
    pub natural_size: Size,
    /// Relative share of remaining main-axis space.
    pub grow: u16,
}

/// Deterministically lays out a row or column into caller-provided storage.
///
/// The function returns how many items were written. When preferred item sizes
/// overflow the bounds, items shrink proportionally after gaps are allocated.
/// When growing items divide extra space unevenly, deterministic remainder
/// distribution ensures every available pixel is assigned.
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

    let available_main = main_size(axis, bounds.size);
    let gap_count = u32::try_from(count.saturating_sub(1)).unwrap_or(u32::MAX);
    let requested_gaps = gap.saturating_mul(gap_count);
    let total_gaps = requested_gaps.min(available_main);
    let item_space = available_main - total_gaps;
    let natural_total = items
        .iter()
        .map(|item| u64::from(main_size(axis, item.natural_size)))
        .fold(0_u64, u64::saturating_add);
    let grow_total = items
        .iter()
        .map(|item| u64::from(item.grow))
        .fold(0_u64, u64::saturating_add);
    let shrinking = natural_total > u64::from(item_space);

    let mut remaining_item_space = item_space;
    let mut remaining_natural = natural_total;
    let mut remaining_extra = u64::from(item_space).saturating_sub(natural_total);
    let mut remaining_grow = grow_total;
    let mut remaining_gap_space = total_gaps;
    let mut remaining_gaps = gap_count;
    let mut cursor = main_origin(axis, bounds);

    for (index, item) in items.iter().enumerate() {
        let natural = main_size(axis, item.natural_size);
        let main = if shrinking {
            proportional_share(remaining_item_space, u64::from(natural), remaining_natural)
        } else if grow_total > 0 {
            natural.saturating_add(proportional_share(
                u32::try_from(remaining_extra).unwrap_or(u32::MAX),
                u64::from(item.grow),
                remaining_grow,
            ))
        } else {
            natural
        };

        output[index] = place_item(axis, bounds, cursor, main, item.natural_size, alignment);
        cursor = cursor.saturating_add(main);

        if shrinking {
            remaining_item_space = remaining_item_space.saturating_sub(main);
            remaining_natural = remaining_natural.saturating_sub(u64::from(natural));
        } else if grow_total > 0 {
            let allocated_extra = main.saturating_sub(natural);
            remaining_extra = remaining_extra.saturating_sub(u64::from(allocated_extra));
            remaining_grow = remaining_grow.saturating_sub(u64::from(item.grow));
        }

        if index + 1 < count {
            let item_gap = proportional_share(remaining_gap_space, 1, u64::from(remaining_gaps));
            cursor = cursor.saturating_add(item_gap);
            remaining_gap_space = remaining_gap_space.saturating_sub(item_gap);
            remaining_gaps = remaining_gaps.saturating_sub(1);
        }
    }
    count
}

fn proportional_share(remaining: u32, weight: u64, remaining_weight: u64) -> u32 {
    if weight == 0 || remaining_weight == 0 {
        return 0;
    }
    let share = u64::from(remaining).saturating_mul(weight) / remaining_weight;
    u32::try_from(share).unwrap_or(u32::MAX)
}

const fn main_size(axis: Axis, size: Size) -> u32 {
    match axis {
        Axis::Horizontal => size.width,
        Axis::Vertical => size.height,
    }
}

const fn main_origin(axis: Axis, bounds: Rect) -> u32 {
    match axis {
        Axis::Horizontal => bounds.origin.x,
        Axis::Vertical => bounds.origin.y,
    }
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
    extern crate std;

    use paper_display::{Rect, Size};
    use proptest::prelude::*;

    use super::{
        Axis, Constraints, CrossAlignment, LayoutError, LinearItem, LogicalSize, ScaleFactor,
        linear_layout,
    };

    #[test]
    fn invalid_constraints_are_rejected_instead_of_panicking() {
        assert_eq!(
            Constraints::new(Size::new(20, 10), Size::new(10, 10)),
            Err(LayoutError::InvalidConstraints {
                min: Size::new(20, 10),
                max: Size::new(10, 10),
            })
        );
    }

    #[test]
    fn scale_factor_resolves_design_units_deterministically() {
        let scale = ScaleFactor::new(3, 2).unwrap();
        assert_eq!(scale.resolve(1), 2);
        assert_eq!(
            scale.resolve_size(LogicalSize::new(20, 10)),
            Size::new(30, 15)
        );
    }

    #[test]
    fn column_distributes_every_extra_pixel_to_growing_items() {
        let items = [
            LinearItem {
                natural_size: Size::new(20, 10),
                grow: 0,
            },
            LinearItem {
                natural_size: Size::new(30, 10),
                grow: 1,
            },
            LinearItem {
                natural_size: Size::new(30, 10),
                grow: 1,
            },
        ];
        let mut output = [Rect::default(); 3];

        linear_layout(
            Axis::Vertical,
            Rect::new(0, 0, 100, 52),
            5,
            CrossAlignment::Center,
            &items,
            &mut output,
        );

        assert_eq!(output[0], Rect::new(40, 0, 20, 10));
        assert_eq!(output[1], Rect::new(35, 15, 30, 16));
        assert_eq!(output[2], Rect::new(35, 36, 30, 16));
        assert_eq!(output[2].bottom(), 52);
    }

    #[test]
    fn overflowing_items_shrink_inside_bounds() {
        let items = [
            LinearItem {
                natural_size: Size::new(8, 5),
                grow: 0,
            },
            LinearItem {
                natural_size: Size::new(8, 5),
                grow: 0,
            },
        ];
        let mut output = [Rect::default(); 2];

        linear_layout(
            Axis::Horizontal,
            Rect::new(10, 0, 10, 5),
            2,
            CrossAlignment::Stretch,
            &items,
            &mut output,
        );

        assert_eq!(output, [Rect::new(10, 0, 4, 5), Rect::new(16, 0, 4, 5)]);
        assert_eq!(output[1].right(), 20);
    }

    proptest! {
        #[test]
        fn linear_layout_never_exceeds_main_axis(
            width in 0_u32..500,
            gap in 0_u32..100,
            first in 0_u32..500,
            second in 0_u32..500,
            first_grow in any::<u16>(),
            second_grow in any::<u16>(),
        ) {
            let items = [
                LinearItem { natural_size: Size::new(first, 10), grow: first_grow },
                LinearItem { natural_size: Size::new(second, 10), grow: second_grow },
            ];
            let mut output = [Rect::default(); 2];
            let bounds = Rect::new(0, 0, width, 10);
            linear_layout(
                Axis::Horizontal,
                bounds,
                gap,
                CrossAlignment::Start,
                &items,
                &mut output,
            );

            prop_assert!(output[0].right() <= bounds.right());
            prop_assert!(output[1].right() <= bounds.right());
        }
    }
}
