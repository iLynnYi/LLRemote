mod geometry;
#[cfg(windows)]
mod windows;

use geometry::{client_size, fit_visible_bounds, outer_for_visible_bounds, Rect};
use std::{collections::HashMap, sync::Mutex};
use tauri::{Manager, PhysicalPosition, PhysicalSize, Window, WindowEvent};

#[derive(Clone, Copy, PartialEq, Eq)]
struct Environment {
    work: Rect,
    scale: u64,
}

#[derive(Default)]
struct State {
    environments: HashMap<String, Environment>,
    pending: HashMap<String, (u64, bool)>,
    next_request: u64,
}

impl State {
    fn enqueue(&mut self, label: &str, force: bool) -> Option<u64> {
        if let Some((_, pending_force)) = self.pending.get_mut(label) {
            *pending_force |= force;
            return None;
        }
        self.next_request = self.next_request.wrapping_add(1);
        self.pending
            .insert(label.to_owned(), (self.next_request, force));
        Some(self.next_request)
    }

    fn take(&mut self, label: &str, request: u64) -> Option<bool> {
        if self.pending.get(label)?.0 != request {
            return None;
        }
        self.pending.remove(label).map(|(_, force)| force)
    }
}

#[derive(Default)]
pub struct WindowBoundsManager(Mutex<State>);

impl WindowBoundsManager {
    pub fn on_event(window: &Window, event: &WindowEvent) {
        let Some(manager) = window.try_state::<Self>() else {
            return;
        };
        match event {
            WindowEvent::Destroyed => {
                if let Ok(mut state) = manager.0.lock() {
                    state.environments.remove(window.label());
                    state.pending.remove(window.label());
                }
            }
            // Do not pin a dragged window to its old monitor. A move only fits
            // after its monitor/work area changes; resizes and DPI changes fit immediately.
            WindowEvent::Moved(_) => Self::request(window, false),
            WindowEvent::Resized(_)
            | WindowEvent::ScaleFactorChanged { .. }
            | WindowEvent::Focused(true) => Self::request(window, true),
            _ => (),
        }
    }

    pub fn request(window: &Window, force: bool) {
        if window.label() != "main" && !window.label().starts_with("session-") {
            return;
        }
        let Some(manager) = window.try_state::<Self>() else {
            return;
        };
        let Ok(mut state) = manager.0.lock() else {
            return;
        };
        let Some(request) = state.enqueue(window.label(), force) else {
            return;
        };
        drop(state);
        let target = window.clone();
        // run_on_main_thread executes immediately when called on the UI thread.
        // Dispatch from the async executor so the current native resize/DPI
        // callback can return before querying or changing the window again.
        tauri::async_runtime::spawn(async move {
            let gui_target = target.clone();
            if target
                .run_on_main_thread(move || run_queued_fit(&gui_target, request))
                .is_err()
            {
                if let Some(manager) = target.try_state::<Self>() {
                    if let Ok(mut state) = manager.0.lock() {
                        state.take(target.label(), request);
                    }
                }
            }
        });
    }
}

fn run_queued_fit(window: &Window, request: u64) {
    let Some(manager) = window.try_state::<WindowBoundsManager>() else {
        return;
    };
    let force = manager
        .0
        .lock()
        .ok()
        .and_then(|mut state| state.take(window.label(), request));
    let Some(force) = force else {
        return;
    };
    if let Err(error) = fit_window(window, force) {
        if let Some(logger) = window.try_state::<crate::app_log::AppLogger>() {
            logger.submit(
                crate::app_log::LogLevel::Warn,
                "window",
                "window_work_area_fit_failed",
                "Unable to fit the app window to its monitor work area.",
                serde_json::json!({ "window": window.label(), "error": error.to_string() }),
            );
        }
    }
}

fn visible_bounds(window: &Window, outer: Rect) -> Rect {
    #[cfg(windows)]
    {
        windows::visible_bounds(window).unwrap_or(outer)
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        outer
    }
}

fn snapshot(window: &Window) -> tauri::Result<(Rect, Rect, (u32, u32))> {
    let position = window.outer_position()?;
    let size = window.outer_size()?;
    let inner = window.inner_size()?;
    let outer = Rect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    Ok((
        outer,
        visible_bounds(window, outer),
        (inner.width, inner.height),
    ))
}

fn fit_window(window: &Window, force: bool) -> tauri::Result<()> {
    // OS-initiated native fullscreen has its own Space/taskbar policy. The app's
    // session expansion uses work-area maximization instead.
    if window.is_minimized()? || window.is_fullscreen()? {
        return Ok(());
    }
    let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) else {
        return Ok(());
    };
    let area = monitor.work_area();
    let work = Rect {
        x: area.position.x,
        y: area.position.y,
        width: area.size.width,
        height: area.size.height,
    };
    if work.width == 0 || work.height == 0 {
        return Ok(());
    }
    let scale = monitor.scale_factor();
    let environment = Environment {
        work,
        scale: scale.to_bits(),
    };
    let manager = window.state::<WindowBoundsManager>();
    let changed = if let Ok(mut state) = manager.0.lock() {
        state
            .environments
            .insert(window.label().to_owned(), environment)
            != Some(environment)
    } else {
        false
    };
    if !force && !changed {
        return Ok(());
    }
    let maximized = window.is_maximized()?;
    if changed {
        let (_, visible, inner) = snapshot(window)?;
        let available = client_size(work, visible, inner);
        let preferred = if window.label() == "main" {
            (900.0, 600.0)
        } else {
            (640.0, 400.0)
        };
        // Minimums are preferences, not a reason to exceed a small logical desktop.
        window.set_min_size(Some(PhysicalSize::new(
            ((preferred.0 * scale).round() as u32).min(available.0),
            ((preferred.1 * scale).round() as u32).min(available.1),
        )))?;
        // Tao's Windows set_min_size internally resizes and clears MAXIMIZED.
        if maximized {
            window.maximize()?;
        }
    }
    let (outer, visible, inner) = snapshot(window)?;
    if visible.width == 0 || visible.height == 0 {
        return Ok(());
    }
    let fitted = fit_visible_bounds(visible, work, maximized);
    if fitted == visible {
        return Ok(());
    }
    let target_outer = outer_for_visible_bounds(outer, visible, fitted);
    #[cfg(windows)]
    if maximized {
        // set_size would unmaximize the window and lose native Restore behavior.
        return windows::set_maximized_bounds(window, target_outer);
    }
    let fitted_inner = client_size(fitted, visible, inner);
    if fitted.width != visible.width || fitted.height != visible.height {
        window.set_size(PhysicalSize::new(fitted_inner.0, fitted_inner.1))?;
    }
    window.set_position(PhysicalPosition::new(target_outer.x, target_outer.y))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::State;

    #[test]
    fn a_resize_upgrades_a_pending_move_without_scheduling_duplicate_work() {
        let mut state = State::default();
        let request = state.enqueue("main", false).unwrap();
        assert_eq!(state.enqueue("main", true), None);
        assert_eq!(state.take("main", request), Some(true));
        assert_eq!(state.take("main", request), None);
    }

    #[test]
    fn queued_work_for_a_destroyed_window_cannot_consume_its_replacements_request() {
        let mut state = State::default();
        let old = state.enqueue("session-one", true).unwrap();
        state.pending.remove("session-one");
        let replacement = state.enqueue("session-one", true).unwrap();
        assert_eq!(state.take("session-one", old), None);
        assert_eq!(state.take("session-one", replacement), Some(true));
    }
}
