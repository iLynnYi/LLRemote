//! Coordinates SCM startup changes without leaving a temporary service running.

pub(super) struct AutoStartReadiness {
    configured: bool,
    credential_configured: bool,
}

impl AutoStartReadiness {
    pub(super) fn new(
        bind_address: Option<&str>,
        port: Option<u16>,
        credential_configured: bool,
    ) -> Self {
        Self {
            configured: bind_address
                .is_some_and(|address| address.parse::<std::net::IpAddr>().is_ok())
                && port.is_some_and(|port| port != 0),
            credential_configured,
        }
    }
}

pub(super) trait AutoStartOperations {
    fn is_running(&mut self) -> Result<bool, String>;
    fn start_configuration_only(&mut self) -> Result<(), String>;
    fn readiness(&mut self) -> Result<AutoStartReadiness, String>;
    fn stop(&mut self) -> Result<(), String>;
    fn set_enabled(&mut self, enabled: bool) -> Result<(), String>;
}

pub(super) fn set_enabled(
    enabled: bool,
    operations: &mut impl AutoStartOperations,
) -> Result<(), String> {
    if !enabled {
        return operations.set_enabled(false);
    }

    let was_running = operations.is_running()?;
    let readiness = if was_running {
        operations.readiness()
    } else {
        operations
            .start_configuration_only()
            .and_then(|()| operations.readiness())
    }
    .and_then(|readiness| {
        if readiness.configured && readiness.credential_configured {
            Ok(())
        } else {
            Err("windows_service_credential_missing".into())
        }
    });

    // Also reap a temporary service whose start timed out. Restore ownership
    // before changing SCM, so any verification/cleanup failure preserves policy.
    if !was_running {
        operations.stop()?;
    }
    readiness?;
    operations.set_enabled(true)
}

#[cfg(test)]
#[path = "../../tests/support/windows_service_auto_start_tests.rs"]
mod tests;
