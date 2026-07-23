use core::fmt;

use paper_display::{DisplayCapabilities, Rect, Waveform};
use paper_graphics::Framebuffer;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RefreshPolicy {
    /// Force a cleanup once this many consecutive partial updates have run.
    pub max_consecutive_partials: u32,
    /// Changes at or above this percentage of the panel use a full refresh.
    pub full_refresh_threshold_percent: u8,
}

impl Default for RefreshPolicy {
    fn default() -> Self {
        Self {
            max_consecutive_partials: 12,
            full_refresh_threshold_percent: 35,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefreshPlan {
    Noop,
    Partial {
        region: Rect,
        waveform: Waveform,
        changed_pixels: usize,
    },
    Full {
        waveform: Waveform,
        changed_pixels: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeError {
    SizeMismatch,
    InvalidPolicy,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SizeMismatch => formatter.write_str("framebuffer sizes do not match"),
            Self::InvalidPolicy => formatter
                .write_str("full_refresh_threshold_percent must be in the inclusive range 1..=100"),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// Stateful, deterministic refresh planner.
#[derive(Clone, Debug)]
pub struct RefreshPlanner {
    policy: RefreshPolicy,
    consecutive_partials: u32,
}

impl RefreshPlanner {
    pub fn new(policy: RefreshPolicy) -> Result<Self, RuntimeError> {
        if !(1..=100).contains(&policy.full_refresh_threshold_percent) {
            return Err(RuntimeError::InvalidPolicy);
        }
        Ok(Self {
            policy,
            consecutive_partials: 0,
        })
    }

    pub const fn consecutive_partials(&self) -> u32 {
        self.consecutive_partials
    }

    pub fn plan(
        &mut self,
        previous: &Framebuffer,
        next: &Framebuffer,
        capabilities: &DisplayCapabilities,
    ) -> Result<RefreshPlan, RuntimeError> {
        if previous.size() != next.size() || next.size() != capabilities.native_size {
            return Err(RuntimeError::SizeMismatch);
        }

        let Some(diff) = diff(previous, next) else {
            return Ok(RefreshPlan::Noop);
        };
        let panel_pixels = next.pixels().len();
        let changed_percent = diff.changed_pixels.saturating_mul(100) / panel_pixels.max(1);

        if !capabilities.partial_updates
            || self.consecutive_partials >= self.policy.max_consecutive_partials
            || changed_percent >= usize::from(self.policy.full_refresh_threshold_percent)
        {
            self.consecutive_partials = 0;
            return Ok(RefreshPlan::Full {
                waveform: Waveform::Grayscale,
                changed_pixels: diff.changed_pixels,
            });
        }

        let monochrome = region_is_monochrome(next, diff.bounds);
        let (region, waveform) = if monochrome {
            let aligned = capabilities
                .fast_monochrome_constraints
                .align_region(diff.bounds, next.size());
            if aligned.contains(diff.bounds.origin)
                && aligned.right() >= diff.bounds.right()
                && aligned.bottom() >= diff.bounds.bottom()
            {
                (aligned, Waveform::FastMonochrome)
            } else {
                (diff.bounds, Waveform::Grayscale)
            }
        } else {
            (diff.bounds, Waveform::Grayscale)
        };

        self.consecutive_partials = self.consecutive_partials.saturating_add(1);
        Ok(RefreshPlan::Partial {
            region,
            waveform,
            changed_pixels: diff.changed_pixels,
        })
    }

    /// Records an externally requested cleanup, resetting ghosting history.
    pub const fn record_cleanup(&mut self) {
        self.consecutive_partials = 0;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Diff {
    bounds: Rect,
    changed_pixels: usize,
}

fn diff(previous: &Framebuffer, next: &Framebuffer) -> Option<Diff> {
    let width = next.size().width as usize;
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut changed_pixels = 0;

    for (index, (&before, &after)) in previous.pixels().iter().zip(next.pixels()).enumerate() {
        if before == after {
            continue;
        }
        changed_pixels += 1;
        let x = u32::try_from(index % width).expect("framebuffer x is bounded by u32 width");
        let y = u32::try_from(index / width).expect("framebuffer y is bounded by u32 height");
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }

    (changed_pixels > 0).then(|| Diff {
        bounds: Rect::new(min_x, min_y, max_x - min_x + 1, max_y - min_y + 1),
        changed_pixels,
    })
}

fn region_is_monochrome(frame: &Framebuffer, region: Rect) -> bool {
    let stride = frame.stride_bytes();
    (region.origin.y..region.bottom()).all(|y| {
        let start = y as usize * stride + region.origin.x as usize;
        let end = start + region.size.width as usize;
        frame.pixels()[start..end]
            .iter()
            .all(|pixel| matches!(pixel, 0 | 255))
    })
}

#[cfg(test)]
mod tests {
    use paper_display::{
        DisplayCapabilities, PixelFormat, Point, Size, UpdateConstraints, Waveform,
    };
    use paper_graphics::{Framebuffer, Gray8};

    use super::{RefreshPlan, RefreshPlanner, RefreshPolicy};

    const FORMATS: &[PixelFormat] = &[PixelFormat::Gray8, PixelFormat::Monochrome1];
    const WAVEFORMS: &[Waveform] = &[
        Waveform::Initialize,
        Waveform::Grayscale,
        Waveform::FastMonochrome,
    ];

    fn capabilities() -> DisplayCapabilities {
        DisplayCapabilities {
            native_size: Size::new(64, 32),
            supported_formats: FORMATS,
            supported_waveforms: WAVEFORMS,
            partial_updates: true,
            fast_monochrome_constraints: UpdateConstraints::new(32, 1, 32, 1),
        }
    }

    #[test]
    fn no_change_is_noop_and_does_not_age_panel() {
        let frame = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut planner = RefreshPlanner::new(RefreshPolicy::default()).unwrap();

        assert_eq!(
            planner.plan(&frame, &frame, &capabilities()).unwrap(),
            RefreshPlan::Noop
        );
        assert_eq!(planner.consecutive_partials(), 0);
    }

    #[test]
    fn monochrome_diff_uses_aligned_fast_update() {
        let before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(33, 4), Gray8::BLACK);
        let mut planner = RefreshPlanner::new(RefreshPolicy::default()).unwrap();

        assert_eq!(
            planner.plan(&before, &after, &capabilities()).unwrap(),
            RefreshPlan::Partial {
                region: paper_display::Rect::new(32, 4, 32, 1),
                waveform: Waveform::FastMonochrome,
                changed_pixels: 1,
            }
        );
    }

    #[test]
    fn grayscale_pixel_uses_quality_waveform() {
        let before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(33, 4), Gray8(127));
        let mut planner = RefreshPlanner::new(RefreshPolicy::default()).unwrap();

        assert!(matches!(
            planner.plan(&before, &after, &capabilities()).unwrap(),
            RefreshPlan::Partial {
                waveform: Waveform::Grayscale,
                ..
            }
        ));
    }

    #[test]
    fn cleanup_is_forced_after_partial_budget() {
        let before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(1, 1), Gray8::BLACK);
        let policy = RefreshPolicy {
            max_consecutive_partials: 0,
            full_refresh_threshold_percent: 100,
        };
        let mut planner = RefreshPlanner::new(policy).unwrap();

        assert!(matches!(
            planner.plan(&before, &after, &capabilities()).unwrap(),
            RefreshPlan::Full { .. }
        ));
    }
}
