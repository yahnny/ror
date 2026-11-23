//! What a hotkey press does, given the current state of the app's windows.
//! Pure logic: no Win32 calls, so it is fully unit-tested.
//!
//! `W` is a window id (an `HWND` in `rord`); any `Copy + Eq` type works.

use std::time::{Duration, Instant};

/// After a launch, further presses don't launch again for this long
/// unless a window has appeared.
pub const LAUNCH_GUARD: Duration = Duration::from_secs(3);

/// The action `rord` should take for one press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action<W> {
    /// Not running, or running with no visible window: run the launch command.
    Launch,
    /// Launched less than [`LAUNCH_GUARD`] ago and no window yet: do nothing.
    AlreadyLaunching,
    /// Has windows, none in front: bring the most recently used one forward.
    Raise(W),
    /// In front with one window: minimize it.
    Minimize(W),
    /// In front with several windows: move to the next one in cycle order.
    Cycle(W),
}

/// Everything [`decide`] needs to know about the app at the moment of a press.
#[derive(Debug, Clone, Copy)]
pub struct Press<'a, W> {
    /// The app's windows in cycle order (see [`CycleOrder`]).
    pub windows: &'a [W],
    /// The app's most recently used window (topmost in Z order).
    pub most_recent: Option<W>,
    /// The foreground window, if it is one of the app's windows.
    pub foreground: Option<W>,
    /// ror launched this app less than [`LAUNCH_GUARD`] ago.
    pub launched_recently: bool,
}

/// Decide what one press does. See "What a hotkey press does" in docs/requirements.md.
pub fn decide<W: Copy + Eq>(p: &Press<'_, W>) -> Action<W> {
    let Some(&first) = p.windows.first() else {
        return if p.launched_recently { Action::AlreadyLaunching } else { Action::Launch };
    };

    if let Some(i) = p.foreground.and_then(|fg| p.windows.iter().position(|&w| w == fg)) {
        return if p.windows.len() == 1 {
            Action::Minimize(p.windows[i])
        } else {
            Action::Cycle(p.windows[(i + 1) % p.windows.len()])
        };
    }

    let target = p.most_recent.filter(|w| p.windows.contains(w)).unwrap_or(first);
    Action::Raise(target)
}

/// True if a launch at `last` is still within [`LAUNCH_GUARD`] at `now`.
pub fn launched_recently(last: Option<Instant>, now: Instant) -> bool {
    last.is_some_and(|t| now.saturating_duration_since(t) < LAUNCH_GUARD)
}

/// A stable cycle order for one app's windows: the order ror first saw them.
#[derive(Debug, Clone)]
pub struct CycleOrder<W> {
    seen: Vec<W>,
}

impl<W> Default for CycleOrder<W> {
    fn default() -> Self {
        CycleOrder { seen: Vec::new() }
    }
}

impl<W: Copy + Eq> CycleOrder<W> {
    /// Update with the app's current windows (in any order) and return the
    /// cycle order: closed windows are dropped, new ones go on the end.
    pub fn update(&mut self, current: &[W]) -> &[W] {
        self.seen.retain(|w| current.contains(w));
        for &w in current {
            if !self.seen.contains(&w) {
                self.seen.push(w);
            }
        }
        &self.seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(windows: &[u32], most_recent: Option<u32>, foreground: Option<u32>) -> Press<'_, u32> {
        Press { windows, most_recent, foreground, launched_recently: false }
    }

    #[test]
    fn not_running_launches() {
        assert_eq!(decide(&press(&[], None, None)), Action::Launch);
    }

    #[test]
    fn recent_launch_without_window_waits() {
        let p = Press { launched_recently: true, ..press(&[], None, None) };
        assert_eq!(decide(&p), Action::AlreadyLaunching);
    }

    #[test]
    fn recent_launch_with_window_raises() {
        let p = Press { launched_recently: true, ..press(&[7], Some(7), None) };
        assert_eq!(decide(&p), Action::Raise(7));
    }

    #[test]
    fn background_app_raises_most_recent_window() {
        assert_eq!(decide(&press(&[1, 2, 3], Some(2), None)), Action::Raise(2));
        // No Z-order info: fall back to the first window.
        assert_eq!(decide(&press(&[1, 2, 3], None, None)), Action::Raise(1));
    }

    #[test]
    fn front_with_one_window_minimizes() {
        assert_eq!(decide(&press(&[5], Some(5), Some(5))), Action::Minimize(5));
    }

    #[test]
    fn front_with_several_windows_cycles_and_wraps() {
        let w = [10, 20, 30];
        assert_eq!(decide(&press(&w, Some(10), Some(10))), Action::Cycle(20));
        assert_eq!(decide(&press(&w, Some(20), Some(20))), Action::Cycle(30));
        assert_eq!(decide(&press(&w, Some(30), Some(30))), Action::Cycle(10));
    }

    #[test]
    fn foreground_from_another_app_is_ignored() {
        assert_eq!(decide(&press(&[1, 2], Some(1), Some(99))), Action::Raise(1));
    }

    #[test]
    fn repeated_presses_visit_every_window() {
        let w = [1, 2, 3, 4];
        let mut fg = 3;
        let mut visited = vec![fg];
        for _ in 0..3 {
            let Action::Cycle(next) = decide(&press(&w, Some(fg), Some(fg))) else { panic!() };
            visited.push(next);
            fg = next;
        }
        visited.sort();
        assert_eq!(visited, w);
    }

    #[test]
    fn launch_guard_window() {
        let t = Instant::now();
        assert!(!launched_recently(None, t));
        assert!(launched_recently(Some(t), t + Duration::from_millis(2900)));
        assert!(!launched_recently(Some(t), t + LAUNCH_GUARD));
    }

    #[test]
    fn cycle_order_is_first_seen_and_stable() {
        let mut c = CycleOrder::default();
        assert_eq!(c.update(&[3, 1]), [3, 1]);
        // Z order changed: cycle order didn't.
        assert_eq!(c.update(&[1, 3]), [3, 1]);
        // New window goes last, closed window drops out.
        assert_eq!(c.update(&[2, 1]), [1, 2]);
        assert_eq!(c.update(&[]), [] as [i32; 0]);
    }
}
