#[cfg(target_os = "macos")]
fn inject_platform_pointer_event(
    button_mask: u8,
    x: u16,
    y: u16,
    last_button_mask: &mut u8,
    shared_display: &SharedDisplay,
) {
    if unsafe { !AXIsProcessTrusted() } {
        return;
    }

    let point = map_vnc_point_to_display(
        x,
        y,
        shared_display.framebuffer,
        shared_display.frame.clone(),
    );
    let cg_point = CGPoint {
        x: point.x as f64,
        y: point.y as f64,
    };

    unsafe {
        post_mouse_event(K_CG_EVENT_MOUSE_MOVED, cg_point, 0);

        if let Some(scroll_delta) =
            scroll_delta_for_pointer_transition(*last_button_mask, button_mask)
        {
            post_scroll_event(scroll_delta);
        }

        for (mask, button, down_event, up_event) in [
            (
                1_u8,
                0_u32,
                K_CG_EVENT_LEFT_MOUSE_DOWN,
                K_CG_EVENT_LEFT_MOUSE_UP,
            ),
            (
                2_u8,
                2_u32,
                K_CG_EVENT_OTHER_MOUSE_DOWN,
                K_CG_EVENT_OTHER_MOUSE_UP,
            ),
            (
                4_u8,
                1_u32,
                K_CG_EVENT_RIGHT_MOUSE_DOWN,
                K_CG_EVENT_RIGHT_MOUSE_UP,
            ),
        ] {
            let was_down = (*last_button_mask & mask) != 0;
            let is_down = (button_mask & mask) != 0;
            if was_down != is_down {
                post_mouse_event(
                    if is_down { down_event } else { up_event },
                    cg_point,
                    button,
                );
            }
        }
    }
}
