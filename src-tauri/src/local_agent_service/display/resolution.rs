pub fn resolve_agent_display_for_request_from_inventory(
    displays: Vec<LocalVncServerDisplay>,
    display_id: Option<&str>,
    framebuffer: Option<(u32, u32)>,
) -> AgentResolvedDisplay {
    let shared = shared_display_for_agent_request_from_inventory(
        displays,
        display_id,
        framebuffer.map(|(width, height)| DisplaySize { width, height }),
    );
    AgentResolvedDisplay {
        id: shared.id,
        x: shared.frame.x,
        width: shared.frame.width,
        framebuffer_width: shared.framebuffer.width,
        framebuffer_height: shared.framebuffer.height,
    }
}

fn shared_display_for_agent_request_from_inventory(
    displays: Vec<LocalVncServerDisplay>,
    display_id: Option<&str>,
    framebuffer: Option<DisplaySize>,
) -> SharedDisplay {
    let selected = display_id
        .filter(|value| !value.trim().is_empty())
        .and_then(|value| displays.iter().find(|display| display.id == value))
        .or_else(|| displays.iter().find(|display| display.is_primary))
        .or_else(|| displays.first());

    if let Some(display) = selected {
        return SharedDisplay {
            relay: None,
            revision: 0,
            desktop_epoch: 0,
            id: display.id.clone(),
            label: display.label.clone(),
            frame: DisplayFrame {
                x: display.x,
                y: display.y,
                width: display.width,
                height: display.height,
                pixel_width: display.pixel_width,
                pixel_height: display.pixel_height,
            },
            framebuffer: framebuffer.unwrap_or(DisplaySize {
                width: display.pixel_width.max(1),
                height: display.pixel_height.max(1),
            }),
            resolution_mode: LocalVncServerResolutionMode::Native,
            refresh_mode: LocalVncServerRefreshMode::Responsive,
        };
    }

    SharedDisplay {
        relay: None,
        revision: 0,
        desktop_epoch: 0,
        id: "main".to_string(),
        label: "Main display".to_string(),
        frame: DisplayFrame {
            x: 0,
            y: 0,
            width: 1280,
            height: 800,
            pixel_width: 1280,
            pixel_height: 800,
        },
        framebuffer: DisplaySize {
            width: 1280,
            height: 800,
        },
        resolution_mode: LocalVncServerResolutionMode::Native,
        refresh_mode: LocalVncServerRefreshMode::Responsive,
    }
}
