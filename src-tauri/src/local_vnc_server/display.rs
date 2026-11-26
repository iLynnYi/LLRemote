fn default_local_vnc_server_resolution_mode() -> LocalVncServerResolutionMode {
    if cfg!(target_os = "windows") {
        LocalVncServerResolutionMode::Native
    } else {
        LocalVncServerResolutionMode::Fit
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SharedDisplay {
    pub(crate) relay: Option<Arc<desktop_relay::DesktopRelay>>,
    pub(crate) revision: u64,
    pub(crate) desktop_epoch: u64,
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) frame: DisplayFrame,
    pub(crate) framebuffer: DisplaySize,
    pub(crate) resolution_mode: LocalVncServerResolutionMode,
    pub(crate) refresh_mode: LocalVncServerRefreshMode,
}

impl SharedDisplay {
    fn source_displays(&self) -> Vec<LocalVncServerDisplay> {
        if self.relay.is_none() {
            return list_local_vnc_server_displays();
        }
        vec![LocalVncServerDisplay {
            id: self.id.clone(),
            label: self.label.clone(),
            kind: LocalVncServerDisplayKind::Main,
            is_primary: true,
            x: 0,
            y: 0,
            width: self.frame.width,
            height: self.frame.height,
            pixel_width: self.frame.pixel_width,
            pixel_height: self.frame.pixel_height,
            scale_factor: 1.0,
        }]
    }
    pub(crate) fn desktop_is_current(&self) -> bool {
        match &self.relay {
            Some(relay) => relay.accepts(self.desktop_epoch),
            None => {
                crate::local_vnc_server_platform::capture_desktop_is_current(self.desktop_epoch)
            }
        }
    }
    /// Replaces the requested view and invalidates in-flight captures, including
    /// switches between displays with identical framebuffer dimensions.
    pub(crate) fn replace_with(&mut self, mut next: Self) -> Result<(), &'static str> {
        next.relay = self.relay.clone();
        next.revision = self
            .revision
            .checked_add(1)
            .ok_or("local_vnc_server_display_revision_overflow")?;
        *self = next;
        Ok(())
    }

    fn fallback_main() -> Self {
        Self {
            relay: None,
            revision: 0,
            desktop_epoch: 0,
            id: "main".to_string(),
            label: "Main display".to_string(),
            frame: DisplayFrame {
                x: 0,
                y: 0,
                width: DEFAULT_FRAMEBUFFER_WIDTH as u32,
                height: DEFAULT_FRAMEBUFFER_HEIGHT as u32,
                pixel_width: DEFAULT_FRAMEBUFFER_WIDTH as u32,
                pixel_height: DEFAULT_FRAMEBUFFER_HEIGHT as u32,
            },
            framebuffer: DisplaySize {
                width: DEFAULT_FRAMEBUFFER_WIDTH as u32,
                height: DEFAULT_FRAMEBUFFER_HEIGHT as u32,
            },
            resolution_mode: LocalVncServerResolutionMode::Fit,
            refresh_mode: DEFAULT_LOCAL_VNC_SERVER_REFRESH_MODE,
        }
    }
}
