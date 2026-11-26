//! Ordinary sharing uses the same repair entry without service-only actions.
use super::{host_requires_write_upgrade, BackgroundShareOverview, BUILD_ID};
use crate::local_vnc_server::LocalVncServerStatus;
use crate::windows_service::{RepairKind, RepairStatus, WindowsServiceRepairItem};

fn item(
    kind: RepairKind,
    status: RepairStatus,
    error_code: Option<String>,
) -> WindowsServiceRepairItem {
    WindowsServiceRepairItem {
        kind,
        status,
        error_code,
    }
}

pub(super) fn run(
    mut snapshot: impl FnMut() -> Result<BackgroundShareOverview, String>,
    ensure: impl FnOnce() -> Result<(), String>,
    restart_idle: impl FnOnce() -> Result<BackgroundShareOverview, String>,
) -> Vec<WindowsServiceRepairItem> {
    let mut changed = false;
    let result = (|| {
        match snapshot() {
            Ok(_) => {}
            Err(code) if code == "background_share_ipc_unavailable" => changed = true,
            Err(code) => return Err(code),
        }
        ensure()?;
        let mut view = snapshot()?;
        let upgrade = host_requires_write_upgrade(&view)?;
        let different_build = view
            .runtime
            .as_ref()
            .is_some_and(|runtime| runtime.build_id != BUILD_ID);
        if upgrade || different_build {
            // Recover checks listener ownership atomically in the host, too.
            // A live listener is never stopped based on a stale status reply.
            if view.server.status != LocalVncServerStatus::Stopped {
                return Err("background_share_runtime_outdated".into());
            }
            view = restart_idle()?;
            changed = true;
        }
        if !view.process_running {
            return Err("background_share_ipc_unavailable".into());
        }
        if host_requires_write_upgrade(&view)?
            || view
                .runtime
                .as_ref()
                .is_some_and(|runtime| runtime.build_id != BUILD_ID)
        {
            return Err("background_share_runtime_outdated".into());
        }
        Ok(view)
    })();
    match result {
        Err(code) => vec![
            item(RepairKind::Background, RepairStatus::Failed, Some(code)),
            item(RepairKind::Agent, RepairStatus::Deferred, None),
        ],
        Ok(view) => {
            let agent = if view.server.status != LocalVncServerStatus::Running {
                item(RepairKind::Agent, RepairStatus::Deferred, None)
            } else if view.server.agent.status
                == crate::local_agent_service::LocalAgentStatus::Running
                && view
                    .server
                    .agent
                    .capabilities
                    .iter()
                    .any(|value| value == "file.transfer.upload")
                && view
                    .server
                    .agent
                    .capabilities
                    .iter()
                    .any(|value| value == "clipboard.text.write")
            {
                item(RepairKind::Agent, RepairStatus::Healthy, None)
            } else {
                item(
                    RepairKind::Agent,
                    RepairStatus::Failed,
                    Some("background_share_enhancement_unavailable".into()),
                )
            };
            vec![
                item(
                    RepairKind::Background,
                    if changed {
                        RepairStatus::Repaired
                    } else {
                        RepairStatus::Healthy
                    },
                    None,
                ),
                agent,
            ]
        }
    }
}
