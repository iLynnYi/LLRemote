/// Desktop loss is itself a transition. Returning to a desktop with the same
/// name still requires a new captured baseline before admitting remote input.
#[derive(Default)]
pub(super) struct DesktopEpoch {
    pub desktop: Option<String>,
    pub epoch: u64,
}

impl DesktopEpoch {
    pub fn observe(&mut self, desktop: Option<String>) -> u64 {
        if self.desktop != desktop {
            self.desktop = desktop;
            self.epoch = self.epoch.saturating_add(1);
        }
        self.epoch
    }

    pub fn accepts(&self, captured_epoch: u64) -> bool {
        self.desktop.is_some() && self.epoch == captured_epoch
    }
}

#[cfg(test)]
#[path = "../../tests/support/windows_desktop_epoch_tests.rs"]
mod tests;
