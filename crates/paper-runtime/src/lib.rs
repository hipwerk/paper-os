//! Pure framebuffer damage analysis and commit-on-success refresh history.

use core::fmt;

use paper_display::{DisplayCapabilities, PixelFormat, Rect, Waveform};
use paper_graphics::Framebuffer;

/// Policy thresholds applied by [`RefreshPlanner`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RefreshPolicy {
    /// Force a cleanup once this many successful partial updates have run.
    pub max_consecutive_partials: u32,
    /// Refreshed regions at or above this panel percentage use a full refresh.
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

/// One semantic refresh decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefreshPlan {
    /// No pixels changed.
    Noop,
    /// Refresh one bounded region.
    Partial {
        /// Final legal region, including required alignment expansion.
        region: Rect,
        /// Semantic waveform required for this update.
        waveform: Waveform,
        /// Target pixel format the backend must produce.
        pixel_format: PixelFormat,
        /// Number of framebuffer pixels whose values changed.
        changed_pixels: usize,
    },
    /// Refresh the complete panel.
    Full {
        /// Semantic waveform required for this update.
        waveform: Waveform,
        /// Target pixel format the backend must produce.
        pixel_format: PixelFormat,
        /// Number of framebuffer pixels whose values changed.
        changed_pixels: usize,
    },
}

/// Refresh-planning failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeError {
    /// The compared frames or display capabilities have different dimensions.
    SizeMismatch,
    /// A policy percentage falls outside `1..=100`.
    InvalidPolicy,
    /// The display does not advertise a safe format/waveform combination.
    UnsupportedCapabilities,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SizeMismatch => formatter.write_str("framebuffer sizes do not match"),
            Self::InvalidPolicy => formatter
                .write_str("full_refresh_threshold_percent must be in the inclusive range 1..=100"),
            Self::UnsupportedCapabilities => formatter
                .write_str("display does not advertise a safe refresh waveform and pixel format"),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// Stateful refresh history with pure planning and explicit success recording.
#[derive(Clone, Debug)]
pub struct RefreshPlanner {
    policy: RefreshPolicy,
    consecutive_partials: u32,
}

impl RefreshPlanner {
    /// Creates a planner after validating its percentage threshold.
    pub fn new(policy: RefreshPolicy) -> Result<Self, RuntimeError> {
        if !(1..=100).contains(&policy.full_refresh_threshold_percent) {
            return Err(RuntimeError::InvalidPolicy);
        }
        Ok(Self {
            policy,
            consecutive_partials: 0,
        })
    }

    /// Returns the number of successful partial updates since the last cleanup.
    pub const fn consecutive_partials(&self) -> u32 {
        self.consecutive_partials
    }

    /// Produces a plan without changing physical-update history.
    ///
    /// Call [`Self::record_success`] only after the display backend confirms the
    /// corresponding update completed.
    pub fn plan(
        &self,
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
        let quality_format =
            quality_format(capabilities).ok_or(RuntimeError::UnsupportedCapabilities)?;
        if !capabilities.supports_waveform(Waveform::Grayscale) {
            return Err(RuntimeError::UnsupportedCapabilities);
        }

        let partial = partial_candidate(previous, next, diff.bounds, capabilities, quality_format);
        let force_full = !capabilities.partial_updates
            || self.consecutive_partials >= self.policy.max_consecutive_partials
            || refreshed_area_at_least(
                partial.region,
                next.size(),
                self.policy.full_refresh_threshold_percent,
            );

        if force_full {
            return Ok(RefreshPlan::Full {
                waveform: Waveform::Grayscale,
                pixel_format: quality_format,
                changed_pixels: diff.changed_pixels,
            });
        }

        Ok(RefreshPlan::Partial {
            region: partial.region,
            waveform: partial.waveform,
            pixel_format: partial.pixel_format,
            changed_pixels: diff.changed_pixels,
        })
    }

    /// Commits panel-aging history after a backend successfully executes `plan`.
    pub fn record_success(&mut self, plan: &RefreshPlan) {
        match plan {
            RefreshPlan::Noop => {}
            RefreshPlan::Partial { .. } => {
                self.consecutive_partials = self.consecutive_partials.saturating_add(1);
            }
            RefreshPlan::Full { .. } => {
                self.consecutive_partials = 0;
            }
        }
    }

    /// Records a successful cleanup requested outside the normal planner.
    pub const fn record_cleanup(&mut self) {
        self.consecutive_partials = 0;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Diff {
    bounds: Rect,
    changed_pixels: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PartialCandidate {
    region: Rect,
    waveform: Waveform,
    pixel_format: PixelFormat,
}

fn partial_candidate(
    previous: &Framebuffer,
    next: &Framebuffer,
    bounds: Rect,
    capabilities: &DisplayCapabilities,
    quality_format: PixelFormat,
) -> PartialCandidate {
    if capabilities.supports_waveform(Waveform::FastMonochrome)
        && capabilities.supports_format(PixelFormat::Monochrome1)
    {
        let aligned = capabilities
            .fast_monochrome_constraints
            .align_region(bounds, next.size());
        if region_covers(aligned, bounds)
            && region_is_binary(previous, aligned)
            && region_is_binary(next, aligned)
        {
            return PartialCandidate {
                region: aligned,
                waveform: Waveform::FastMonochrome,
                pixel_format: PixelFormat::Monochrome1,
            };
        }
    }

    PartialCandidate {
        region: bounds,
        waveform: Waveform::Grayscale,
        pixel_format: quality_format,
    }
}

fn quality_format(capabilities: &DisplayCapabilities) -> Option<PixelFormat> {
    [
        PixelFormat::Gray8,
        PixelFormat::Gray4,
        PixelFormat::Gray2,
        PixelFormat::Monochrome1,
    ]
    .into_iter()
    .find(|format| capabilities.supports_format(*format))
}

const fn region_covers(outer: Rect, inner: Rect) -> bool {
    !outer.is_empty()
        && outer.origin.x <= inner.origin.x
        && outer.origin.y <= inner.origin.y
        && outer.right() >= inner.right()
        && outer.bottom() >= inner.bottom()
}

fn refreshed_area_at_least(region: Rect, panel: paper_display::Size, threshold: u8) -> bool {
    let refreshed = u128::from(region.size.width) * u128::from(region.size.height) * 100;
    let panel = u128::from(panel.width) * u128::from(panel.height) * u128::from(threshold);
    refreshed >= panel
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

fn region_is_binary(frame: &Framebuffer, region: Rect) -> bool {
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

    use super::{RefreshPlan, RefreshPlanner, RefreshPolicy, RuntimeError};

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
        let planner = RefreshPlanner::new(RefreshPolicy::default()).unwrap();

        assert_eq!(
            planner.plan(&frame, &frame, &capabilities()).unwrap(),
            RefreshPlan::Noop
        );
        assert_eq!(planner.consecutive_partials(), 0);
    }

    #[test]
    fn planning_is_pure_until_success_is_recorded() {
        let before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(33, 4), Gray8::BLACK);
        let mut planner = RefreshPlanner::new(RefreshPolicy::default()).unwrap();

        let first = planner.plan(&before, &after, &capabilities()).unwrap();
        let second = planner.plan(&before, &after, &capabilities()).unwrap();
        assert_eq!(first, second);
        assert_eq!(planner.consecutive_partials(), 0);

        planner.record_success(&first);
        assert_eq!(planner.consecutive_partials(), 1);
    }

    #[test]
    fn monochrome_transition_uses_aligned_fast_update() {
        let before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(33, 4), Gray8::BLACK);
        let planner = RefreshPlanner::new(RefreshPolicy::default()).unwrap();

        assert_eq!(
            planner.plan(&before, &after, &capabilities()).unwrap(),
            RefreshPlan::Partial {
                region: paper_display::Rect::new(32, 4, 32, 1),
                waveform: Waveform::FastMonochrome,
                pixel_format: PixelFormat::Monochrome1,
                changed_pixels: 1,
            }
        );
    }

    #[test]
    fn grayscale_to_white_transition_does_not_use_fast_waveform() {
        let mut before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        before.set(Point::new(33, 4), Gray8(127));
        let after = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let planner = RefreshPlanner::new(RefreshPolicy::default()).unwrap();

        assert!(matches!(
            planner.plan(&before, &after, &capabilities()).unwrap(),
            RefreshPlan::Partial {
                waveform: Waveform::Grayscale,
                pixel_format: PixelFormat::Gray8,
                ..
            }
        ));
    }

    #[test]
    fn grayscale_inside_aligned_region_prevents_fast_waveform() {
        let mut before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        before.set(Point::new(40, 4), Gray8(127));
        let mut after = before.clone();
        after.set(Point::new(33, 4), Gray8::BLACK);
        let planner = RefreshPlanner::new(RefreshPolicy::default()).unwrap();

        assert!(matches!(
            planner.plan(&before, &after, &capabilities()).unwrap(),
            RefreshPlan::Partial {
                region: paper_display::Rect {
                    origin: Point { x: 33, y: 4 },
                    ..
                },
                waveform: Waveform::Grayscale,
                ..
            }
        ));
    }

    #[test]
    fn sparse_changes_with_panel_sized_bounds_force_full_refresh() {
        let before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(0, 0), Gray8::BLACK);
        after.set(Point::new(63, 31), Gray8::BLACK);
        let planner = RefreshPlanner::new(RefreshPolicy::default()).unwrap();

        assert!(matches!(
            planner.plan(&before, &after, &capabilities()).unwrap(),
            RefreshPlan::Full {
                changed_pixels: 2,
                ..
            }
        ));
    }

    #[test]
    fn unsupported_quality_waveform_is_rejected() {
        const FAST_ONLY: &[Waveform] = &[Waveform::FastMonochrome];
        let capabilities = DisplayCapabilities {
            supported_waveforms: FAST_ONLY,
            ..capabilities()
        };
        let before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(1, 1), Gray8::BLACK);
        let planner = RefreshPlanner::new(RefreshPolicy::default()).unwrap();

        assert_eq!(
            planner.plan(&before, &after, &capabilities),
            Err(RuntimeError::UnsupportedCapabilities)
        );
    }

    #[test]
    fn cleanup_is_forced_after_successful_partial_budget() {
        let before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(1, 1), Gray8::BLACK);
        let policy = RefreshPolicy {
            max_consecutive_partials: 1,
            full_refresh_threshold_percent: 100,
        };
        let mut planner = RefreshPlanner::new(policy).unwrap();
        let first = planner.plan(&before, &after, &capabilities()).unwrap();
        assert!(matches!(first, RefreshPlan::Partial { .. }));
        planner.record_success(&first);

        assert!(matches!(
            planner.plan(&before, &after, &capabilities()).unwrap(),
            RefreshPlan::Full { .. }
        ));
    }
}
