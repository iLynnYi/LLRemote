/// Per-rectangle color validity shared by incremental framing and pixel decode.
#[derive(Clone, Copy, Default)]
pub(super) struct HextileColorState {
    background: bool,
    foreground: bool,
}

impl HextileColorState {
    pub(super) fn begin_tile(&mut self, flags: u8) -> Result<(), String> {
        if flags & 1 != 0 {
            *self = Self::default();
            return Ok(());
        }
        if flags & 4 != 0 && flags & 16 != 0 {
            return Err("rfb_hextile_invalid_flags".into());
        }
        self.background |= flags & 2 != 0;
        if !self.background {
            return Err("rfb_hextile_background_missing".into());
        }
        if flags & 16 != 0 {
            self.foreground = false;
        } else {
            self.foreground |= flags & 4 != 0;
        }
        Ok(())
    }

    pub(super) fn check_subrectangles(&self, flags: u8, count: u8) -> Result<(), String> {
        if count != 0 && flags & 16 == 0 && !self.foreground {
            return Err("rfb_hextile_foreground_missing".into());
        }
        Ok(())
    }
}
