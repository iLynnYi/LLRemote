#[derive(Debug, PartialEq)]
pub(super) struct PopupLimits {
    pub width: i32,
    pub height: i32,
}

/// Outer bounds, including the native title bar, stay within half the main window.
pub(super) fn popup_limits(main: Option<(i32, i32)>, work: (i32, i32), scale: f64) -> PopupLimits {
    let reference = main.filter(|(w, h)| *w > 0 && *h > 0).unwrap_or((
        (1280.0 * scale).round() as i32,
        (768.0 * scale).round() as i32,
    ));
    let margin = (24.0 * scale).round() as i32;
    PopupLimits {
        width: (reference.0 / 2).min(work.0.saturating_sub(margin)).max(1),
        height: (reference.1 / 2).min(work.1.saturating_sub(margin)).max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respects_half_of_the_actual_main_window_including_on_high_dpi() {
        assert_eq!(
            popup_limits(Some((1280, 768)), (1920, 1040), 1.0),
            PopupLimits {
                width: 640,
                height: 384
            }
        );
        assert_eq!(
            popup_limits(Some((1800, 1200)), (3840, 2100), 2.0),
            PopupLimits {
                width: 900,
                height: 600
            }
        );
    }

    #[test]
    fn background_only_mode_uses_the_apps_default_main_window_and_fits_the_work_area() {
        assert_eq!(
            popup_limits(None, (1920, 1040), 1.0),
            PopupLimits {
                width: 640,
                height: 384
            }
        );
        assert_eq!(
            popup_limits(None, (3840, 2100), 2.0),
            PopupLimits {
                width: 1280,
                height: 768
            }
        );
        assert_eq!(
            popup_limits(Some((3000, 2000)), (800, 600), 1.0),
            PopupLimits {
                width: 776,
                height: 576
            }
        );
    }
}
