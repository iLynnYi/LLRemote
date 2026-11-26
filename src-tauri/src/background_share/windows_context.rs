//! Desktop identity and launch strategy, independent of Windows handle APIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LaunchMode {
    InheritDesktop,
    RestrictedToken,
}

pub(super) struct UserContext<'a> {
    pub sid: &'a str,
    pub session: u32,
    pub elevated: bool,
}

pub(super) fn launch_mode(
    caller: &UserContext<'_>,
    desktop: &UserContext<'_>,
) -> Result<LaunchMode, String> {
    if caller.sid != desktop.sid {
        return Err("background_share_access_denied".into());
    }
    if caller.sid == "S-1-5-18" || caller.session == 0 || desktop.session != caller.session {
        return Err("background_share_user_context_required".into());
    }
    // Only the desktop's inherited identity determines whether an explicit
    // restricted token is needed. An elevated caller is not itself a failure.
    Ok(if desktop.elevated {
        LaunchMode::RestrictedToken
    } else {
        LaunchMode::InheritDesktop
    })
}

#[cfg(test)]
#[path = "../../tests/support/background_share_windows_context_tests.rs"]
mod tests;
