#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub(super) fn fit_visible_bounds(current: Rect, work: Rect, expanded: bool) -> Rect {
    if expanded {
        return work;
    }
    let width = current.width.min(work.width);
    let height = current.height.min(work.height);
    Rect {
        x: (current.x as i64).clamp(work.x as i64, work.x as i64 + (work.width - width) as i64)
            as i32,
        y: (current.y as i64).clamp(work.y as i64, work.y as i64 + (work.height - height) as i64)
            as i32,
        width,
        height,
    }
}

/// Keep native invisible resize borders outside the visible frame. Comparing
/// GetWindowRect alone incorrectly treats normal maximized borders as overflow.
pub(super) fn outer_for_visible_bounds(outer: Rect, visible: Rect, target: Rect) -> Rect {
    Rect {
        x: (outer.x as i64 + target.x as i64 - visible.x as i64) as i32,
        y: (outer.y as i64 + target.y as i64 - visible.y as i64) as i32,
        width: target
            .width
            .saturating_add(outer.width.saturating_sub(visible.width)),
        height: target
            .height
            .saturating_add(outer.height.saturating_sub(visible.height)),
    }
}

/// Tauri set_size takes the client size; work areas and window bounds include
/// title bars. All values here are physical pixels, including on Retina/HiDPI.
pub(super) fn client_size(target: Rect, visible: Rect, inner: (u32, u32)) -> (u32, u32) {
    (
        target
            .width
            .saturating_sub(visible.width.saturating_sub(inner.0))
            .max(1),
        target
            .height
            .saturating_sub(visible.height.saturating_sub(inner.1))
            .max(1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn oversized_main_window_fits_scaled_desktop_including_title_bar() {
        let visible = rect(0, 0, 2560, 1600);
        let work = rect(0, 0, 1920, 1032);
        let fitted = fit_visible_bounds(visible, work, false);
        assert_eq!(fitted, work);
        assert_eq!(client_size(fitted, visible, (2560, 1536)), (1920, 968));
    }

    #[test]
    fn expanded_window_excludes_taskbar_but_preserves_invisible_resize_borders() {
        let outer = rect(-8, -8, 1936, 1096);
        let visible = rect(0, 0, 1920, 1080);
        let work = rect(0, 0, 1920, 1032);
        let fitted = fit_visible_bounds(visible, work, true);
        assert_eq!(
            outer_for_visible_bounds(outer, visible, fitted),
            rect(-8, -8, 1936, 1048)
        );
        assert_eq!(fitted, work);
    }

    #[test]
    fn menu_bar_and_side_dock_on_negative_origin_monitor_are_preserved() {
        let work = rect(-1840, 48, 1840, 1000);
        assert_eq!(
            fit_visible_bounds(rect(-1920, 0, 1920, 1080), work, true),
            work
        );
        assert_eq!(
            fit_visible_bounds(rect(-2000, 0, 1000, 600), work, false),
            rect(-1840, 48, 1000, 600)
        );
    }

    #[test]
    fn moving_to_smaller_work_area_repositions_without_growing_normal_window() {
        let work = rect(1920, 24, 1280, 696);
        assert_eq!(
            fit_visible_bounds(rect(3000, 500, 900, 500), work, false),
            rect(2300, 220, 900, 500)
        );
    }

    #[test]
    fn fitting_is_idempotent_and_small_work_areas_override_preferred_minimum() {
        let work = rect(0, 24, 600, 350);
        let fitted = fit_visible_bounds(rect(0, 0, 1280, 768), work, false);
        assert_eq!(fit_visible_bounds(fitted, work, false), fitted);
        assert_eq!(
            client_size(work, rect(0, 0, 1280, 768), (1280, 736)),
            (600, 318)
        );
    }
}
