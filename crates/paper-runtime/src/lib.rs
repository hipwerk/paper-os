//! Transactional framebuffer damage analysis and refresh history.

use core::fmt;
use core::sync::atomic::{AtomicUsize, Ordering};

use paper_display::{DisplayCapabilities, PixelFormat, Rect, UpdateProfile, Waveform};
use paper_graphics::Framebuffer;

static NEXT_RUNTIME_ID: AtomicUsize = AtomicUsize::new(1);

/// Policy thresholds applied by [`RefreshRuntime`].
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

/// A plan bound to the exact framebuffer generation that produced it.
///
/// The value is intentionally neither cloneable nor publicly constructible.
/// Pass its plan and framebuffer to a backend, then move it into
/// [`RefreshRuntime::commit_success`] only after the physical operation
/// completes.
#[derive(Debug)]
pub struct PendingRefresh {
    runtime_id: usize,
    generation: u64,
    plan: RefreshPlan,
    next: Framebuffer,
}

impl PendingRefresh {
    /// Returns the semantic operation to execute.
    pub const fn plan(&self) -> RefreshPlan {
        self.plan
    }

    /// Returns the framebuffer associated with the operation.
    pub const fn framebuffer(&self) -> &Framebuffer {
        &self.next
    }
}

/// Refresh-planning or commit failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeError {
    /// The next frame or display capabilities have different dimensions.
    SizeMismatch,
    /// A policy percentage falls outside `1..=100`.
    InvalidPolicy,
    /// The display does not advertise a safe update profile.
    UnsupportedCapabilities,
    /// Another commit or uncertain failure invalidated this pending update.
    StalePendingRefresh,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SizeMismatch => formatter.write_str("framebuffer sizes do not match"),
            Self::InvalidPolicy => formatter
                .write_str("full_refresh_threshold_percent must be in the inclusive range 1..=100"),
            Self::UnsupportedCapabilities => {
                formatter.write_str("display does not advertise a safe update profile")
            }
            Self::StalePendingRefresh => {
                formatter.write_str("pending refresh no longer matches runtime state")
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

/// Framebuffer reference and panel-aging state.
#[derive(Debug)]
pub struct RefreshRuntime {
    runtime_id: usize,
    policy: RefreshPolicy,
    previous: Framebuffer,
    consecutive_partials: u32,
    generation: u64,
    panel_state_uncertain: bool,
}

impl RefreshRuntime {
    /// Creates runtime state without assuming what is currently on the panel.
    ///
    /// The supplied framebuffer is a reference for diagnostics only until one
    /// full update succeeds. This is the safe constructor for cold starts and
    /// processes that have not restored trustworthy persisted state.
    pub fn new(reference: Framebuffer, policy: RefreshPolicy) -> Result<Self, RuntimeError> {
        if !(1..=100).contains(&policy.full_refresh_threshold_percent) {
            return Err(RuntimeError::InvalidPolicy);
        }
        Ok(Self {
            runtime_id: NEXT_RUNTIME_ID.fetch_add(1, Ordering::Relaxed),
            policy,
            previous: reference,
            consecutive_partials: 0,
            generation: 0,
            panel_state_uncertain: true,
        })
    }

    /// Restores state known to match the persistent panel.
    ///
    /// Use this only after validating that both the framebuffer and partial
    /// refresh count were persisted after confirmed backend success.
    pub fn from_known_panel_state(
        previous: Framebuffer,
        consecutive_partials: u32,
        policy: RefreshPolicy,
    ) -> Result<Self, RuntimeError> {
        let mut runtime = Self::new(previous, policy)?;
        runtime.consecutive_partials = consecutive_partials;
        runtime.panel_state_uncertain = false;
        Ok(runtime)
    }

    /// Returns the framebuffer currently used as the committed-state reference.
    ///
    /// It is authoritative only when [`Self::panel_state_uncertain`] is false.
    pub const fn previous_frame(&self) -> &Framebuffer {
        &self.previous
    }

    /// Returns the number of successful partial updates since the last cleanup.
    pub const fn consecutive_partials(&self) -> u32 {
        self.consecutive_partials
    }

    /// Returns whether the next plan must re-establish the complete panel.
    pub const fn panel_state_uncertain(&self) -> bool {
        self.panel_state_uncertain
    }

    /// Produces an opaque pending update without changing committed history.
    pub fn plan(
        &self,
        next: Framebuffer,
        capabilities: &DisplayCapabilities,
    ) -> Result<PendingRefresh, RuntimeError> {
        if self.previous.size() != next.size() || next.size() != capabilities.native_size {
            return Err(RuntimeError::SizeMismatch);
        }
        let plan = plan_frames(
            &self.previous,
            &next,
            capabilities,
            self.policy,
            self.consecutive_partials,
            self.panel_state_uncertain,
        )?;
        Ok(PendingRefresh {
            runtime_id: self.runtime_id,
            generation: self.generation,
            plan,
            next,
        })
    }

    /// Atomically commits framebuffer and panel-aging history after success.
    pub fn commit_success(&mut self, pending: PendingRefresh) -> Result<(), RuntimeError> {
        if pending.runtime_id != self.runtime_id
            || pending.generation != self.generation
            || pending.next.size() != self.previous.size()
        {
            return Err(RuntimeError::StalePendingRefresh);
        }

        match pending.plan {
            RefreshPlan::Noop => {}
            RefreshPlan::Partial { .. } => {
                self.consecutive_partials = self.consecutive_partials.saturating_add(1);
            }
            RefreshPlan::Full { .. } => {
                self.consecutive_partials = 0;
                self.panel_state_uncertain = false;
            }
        }
        self.previous = pending.next;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }

    /// Invalidates outstanding plans and forces the next operation to be full.
    ///
    /// Call this when a backend failure may have reached the panel but did not
    /// provide a trustworthy completion result.
    pub fn mark_panel_state_uncertain(&mut self) {
        self.panel_state_uncertain = true;
        self.generation = self.generation.wrapping_add(1);
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
    profile: UpdateProfile,
}

fn plan_frames(
    previous: &Framebuffer,
    next: &Framebuffer,
    capabilities: &DisplayCapabilities,
    policy: RefreshPolicy,
    consecutive_partials: u32,
    panel_state_uncertain: bool,
) -> Result<RefreshPlan, RuntimeError> {
    let difference = diff(previous, next);
    if difference.is_none() && !panel_state_uncertain {
        return Ok(RefreshPlan::Noop);
    }

    let quality = quality_profile(capabilities).ok_or(RuntimeError::UnsupportedCapabilities)?;
    if panel_state_uncertain {
        return Ok(RefreshPlan::Full {
            waveform: quality.waveform(),
            pixel_format: quality.pixel_format(),
            changed_pixels: difference.map_or(0, |diff| diff.changed_pixels),
        });
    }

    let difference = difference.expect("unchanged frames returned above");
    let partial = partial_candidate(previous, next, difference.bounds, capabilities, quality);
    let force_full = partial.is_none_or(|candidate| {
        consecutive_partials >= policy.max_consecutive_partials
            || refreshed_area_at_least(
                candidate.region,
                next.size(),
                policy.full_refresh_threshold_percent,
            )
    });

    if force_full {
        return Ok(RefreshPlan::Full {
            waveform: quality.waveform(),
            pixel_format: quality.pixel_format(),
            changed_pixels: difference.changed_pixels,
        });
    }

    let partial = partial.expect("partial candidate was checked above");
    Ok(RefreshPlan::Partial {
        region: partial.region,
        waveform: partial.profile.waveform(),
        pixel_format: partial.profile.pixel_format(),
        changed_pixels: difference.changed_pixels,
    })
}

fn partial_candidate(
    previous: &Framebuffer,
    next: &Framebuffer,
    bounds: Rect,
    capabilities: &DisplayCapabilities,
    quality: UpdateProfile,
) -> Option<PartialCandidate> {
    if let Some(fast) = capabilities.profile(PixelFormat::Monochrome1, Waveform::FastMonochrome) {
        let aligned = fast.constraints().align_region(bounds, next.size());
        if fast.supports_partial()
            && region_covers(aligned, bounds)
            && region_is_binary(previous, aligned)
            && region_is_binary(next, aligned)
        {
            return Some(PartialCandidate {
                region: aligned,
                profile: fast,
            });
        }
    }

    let aligned = quality.constraints().align_region(bounds, next.size());
    (quality.supports_partial() && region_covers(aligned, bounds)).then_some(PartialCandidate {
        region: aligned,
        profile: quality,
    })
}

fn quality_profile(capabilities: &DisplayCapabilities) -> Option<UpdateProfile> {
    [
        PixelFormat::Gray8,
        PixelFormat::Gray4,
        PixelFormat::Gray2,
        PixelFormat::Monochrome1,
    ]
    .into_iter()
    .find_map(|format| capabilities.profile(format, Waveform::Grayscale))
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
        DisplayCapabilities, PixelFormat, Point, Rect, Size, UpdateConstraints, UpdateProfile,
        Waveform,
    };
    use paper_graphics::{Framebuffer, Gray8};
    use proptest::prelude::*;

    use super::{RefreshPlan, RefreshPolicy, RefreshRuntime, RuntimeError};

    const PROFILES: &[UpdateProfile] = &[
        UpdateProfile::new(
            PixelFormat::Gray8,
            Waveform::Grayscale,
            true,
            UpdateConstraints::UNRESTRICTED,
        ),
        UpdateProfile::new(
            PixelFormat::Monochrome1,
            Waveform::FastMonochrome,
            true,
            UpdateConstraints::new(32, 1, 32, 1).expect("test profile alignments are non-zero"),
        ),
    ];

    fn capabilities(size: Size) -> DisplayCapabilities {
        DisplayCapabilities {
            native_size: size,
            update_profiles: PROFILES,
        }
    }

    fn runtime(frame: Framebuffer) -> RefreshRuntime {
        RefreshRuntime::from_known_panel_state(frame, 0, RefreshPolicy::default()).unwrap()
    }

    #[test]
    fn cold_start_forces_full_update_even_when_reference_matches() {
        let frame = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut runtime = RefreshRuntime::new(frame.clone(), RefreshPolicy::default()).unwrap();

        assert!(runtime.panel_state_uncertain());
        let pending = runtime
            .plan(frame.clone(), &capabilities(frame.size()))
            .unwrap();
        assert!(matches!(
            pending.plan(),
            RefreshPlan::Full {
                changed_pixels: 0,
                ..
            }
        ));

        runtime.commit_success(pending).unwrap();
        assert!(!runtime.panel_state_uncertain());
        assert_eq!(runtime.previous_frame(), &frame);
    }

    #[test]
    fn no_change_is_noop_and_commits_identical_frame() {
        let frame = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut runtime = runtime(frame.clone());
        let pending = runtime
            .plan(frame.clone(), &capabilities(frame.size()))
            .unwrap();
        assert_eq!(pending.plan(), RefreshPlan::Noop);
        runtime.commit_success(pending).unwrap();
        assert_eq!(runtime.previous_frame(), &frame);
        assert_eq!(runtime.consecutive_partials(), 0);
    }

    #[test]
    fn planning_is_pure_and_commit_advances_frame_and_history_together() {
        let before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(33, 4), Gray8::BLACK);
        let mut runtime = runtime(before.clone());

        let first = runtime
            .plan(after.clone(), &capabilities(after.size()))
            .unwrap();
        let second = runtime
            .plan(after.clone(), &capabilities(after.size()))
            .unwrap();
        assert_eq!(first.plan(), second.plan());
        assert_eq!(runtime.previous_frame(), &before);
        assert_eq!(runtime.consecutive_partials(), 0);

        runtime.commit_success(first).unwrap();
        assert_eq!(runtime.previous_frame(), &after);
        assert_eq!(runtime.consecutive_partials(), 1);
        assert_eq!(
            runtime.commit_success(second),
            Err(RuntimeError::StalePendingRefresh)
        );
    }

    #[test]
    fn pending_update_cannot_commit_to_a_different_runtime() {
        let before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(1, 1), Gray8::BLACK);
        let source = runtime(before.clone());
        let pending = source
            .plan(after.clone(), &capabilities(after.size()))
            .unwrap();
        let mut other = runtime(before);

        assert_eq!(
            other.commit_success(pending),
            Err(RuntimeError::StalePendingRefresh)
        );
        assert_eq!(
            other.previous_frame().get(Point::new(1, 1)),
            Some(Gray8::WHITE)
        );
    }

    #[test]
    fn monochrome_transition_uses_aligned_fast_update() {
        let before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(33, 4), Gray8::BLACK);
        let runtime = runtime(before);

        assert_eq!(
            runtime
                .plan(after.clone(), &capabilities(after.size()))
                .unwrap()
                .plan(),
            RefreshPlan::Partial {
                region: Rect::new(32, 4, 32, 1),
                waveform: Waveform::FastMonochrome,
                pixel_format: PixelFormat::Monochrome1,
                changed_pixels: 1,
            }
        );
    }

    #[test]
    fn grayscale_to_white_transition_uses_quality_profile() {
        let mut before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        before.set(Point::new(33, 4), Gray8(127));
        let after = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let runtime = runtime(before);

        assert!(matches!(
            runtime
                .plan(after.clone(), &capabilities(after.size()))
                .unwrap()
                .plan(),
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
        let runtime = runtime(before);

        assert!(matches!(
            runtime
                .plan(after.clone(), &capabilities(after.size()))
                .unwrap()
                .plan(),
            RefreshPlan::Partial {
                region: Rect {
                    origin: Point { x: 33, y: 4 },
                    ..
                },
                waveform: Waveform::Grayscale,
                ..
            }
        ));
    }

    #[test]
    fn unalignable_panel_edge_falls_back_without_dropping_pixels() {
        let size = Size::new(65, 1);
        let before = Framebuffer::new(size, Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(64, 0), Gray8::BLACK);
        let runtime = runtime(before);

        assert_eq!(
            runtime.plan(after, &capabilities(size)).unwrap().plan(),
            RefreshPlan::Partial {
                region: Rect::new(64, 0, 1, 1),
                waveform: Waveform::Grayscale,
                pixel_format: PixelFormat::Gray8,
                changed_pixels: 1,
            }
        );
    }

    #[test]
    fn sparse_changes_with_panel_sized_bounds_force_full_refresh() {
        let before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(0, 0), Gray8::BLACK);
        after.set(Point::new(63, 31), Gray8::BLACK);
        let runtime = runtime(before);

        assert!(matches!(
            runtime
                .plan(after.clone(), &capabilities(after.size()))
                .unwrap()
                .plan(),
            RefreshPlan::Full {
                changed_pixels: 2,
                ..
            }
        ));
    }

    #[test]
    fn unsupported_quality_profile_is_rejected() {
        const FAST_ONLY: &[UpdateProfile] = &[UpdateProfile::new(
            PixelFormat::Monochrome1,
            Waveform::FastMonochrome,
            true,
            UpdateConstraints::UNRESTRICTED,
        )];
        let size = Size::new(64, 32);
        let capabilities = DisplayCapabilities {
            native_size: size,
            update_profiles: FAST_ONLY,
        };
        let before = Framebuffer::new(size, Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(1, 1), Gray8::BLACK);
        let runtime = runtime(before);

        assert!(matches!(
            runtime.plan(after, &capabilities),
            Err(RuntimeError::UnsupportedCapabilities)
        ));
    }

    #[test]
    fn full_only_quality_profile_never_produces_partial_plan() {
        const FULL_ONLY: &[UpdateProfile] = &[UpdateProfile::new(
            PixelFormat::Gray8,
            Waveform::Grayscale,
            false,
            UpdateConstraints::UNRESTRICTED,
        )];
        let size = Size::new(64, 32);
        let capabilities = DisplayCapabilities {
            native_size: size,
            update_profiles: FULL_ONLY,
        };
        let before = Framebuffer::new(size, Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(1, 1), Gray8::BLACK);
        let runtime = runtime(before);

        assert!(matches!(
            runtime.plan(after, &capabilities).unwrap().plan(),
            RefreshPlan::Full { .. }
        ));
    }

    #[test]
    fn cleanup_is_forced_after_committed_partial_budget() {
        let before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(1, 1), Gray8::BLACK);
        let policy = RefreshPolicy {
            max_consecutive_partials: 1,
            full_refresh_threshold_percent: 100,
        };
        let mut runtime = RefreshRuntime::from_known_panel_state(before, 0, policy).unwrap();
        let first = runtime
            .plan(after.clone(), &capabilities(after.size()))
            .unwrap();
        assert!(matches!(first.plan(), RefreshPlan::Partial { .. }));
        runtime.commit_success(first).unwrap();

        let mut next = after;
        next.set(Point::new(2, 2), Gray8::BLACK);
        assert!(matches!(
            runtime
                .plan(next.clone(), &capabilities(next.size()))
                .unwrap()
                .plan(),
            RefreshPlan::Full { .. }
        ));
    }

    #[test]
    fn restored_partial_history_cannot_bypass_cleanup_budget() {
        let before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(1, 1), Gray8::BLACK);
        let policy = RefreshPolicy {
            max_consecutive_partials: 3,
            full_refresh_threshold_percent: 100,
        };
        let runtime = RefreshRuntime::from_known_panel_state(before, 3, policy).unwrap();

        assert_eq!(runtime.consecutive_partials(), 3);
        assert!(matches!(
            runtime
                .plan(after.clone(), &capabilities(after.size()))
                .unwrap()
                .plan(),
            RefreshPlan::Full { .. }
        ));
    }

    #[test]
    fn uncertain_failure_invalidates_pending_and_forces_full_cleanup() {
        let before = Framebuffer::new(Size::new(64, 32), Gray8::WHITE).unwrap();
        let mut after = before.clone();
        after.set(Point::new(1, 1), Gray8::BLACK);
        let mut runtime = runtime(before);
        let stale = runtime
            .plan(after.clone(), &capabilities(after.size()))
            .unwrap();

        runtime.mark_panel_state_uncertain();
        assert!(runtime.panel_state_uncertain());
        assert_eq!(
            runtime.commit_success(stale),
            Err(RuntimeError::StalePendingRefresh)
        );

        let cleanup = runtime
            .plan(after.clone(), &capabilities(after.size()))
            .unwrap();
        assert!(matches!(cleanup.plan(), RefreshPlan::Full { .. }));
        runtime.commit_success(cleanup).unwrap();
        assert!(!runtime.panel_state_uncertain());
        assert_eq!(runtime.previous_frame(), &after);
    }

    proptest! {
        #[test]
        fn every_partial_plan_covers_every_changed_pixel(
            width in 1_u32..80,
            height in 1_u32..60,
            first_x in any::<u32>(),
            first_y in any::<u32>(),
            second_x in any::<u32>(),
            second_y in any::<u32>(),
        ) {
            let size = Size::new(width, height);
            let before = Framebuffer::new(size, Gray8::WHITE).unwrap();
            let mut after = before.clone();
            let first = Point::new(first_x % width, first_y % height);
            let second = Point::new(second_x % width, second_y % height);
            after.set(first, Gray8::BLACK);
            after.set(second, Gray8::BLACK);
            let runtime = runtime(before);
            let pending = runtime.plan(after, &capabilities(size)).unwrap();

            if let RefreshPlan::Partial { region, .. } = pending.plan() {
                prop_assert!(region.contains(first));
                prop_assert!(region.contains(second));
            }
        }
    }
}
