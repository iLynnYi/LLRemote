//! One Windows GUI per user and login session, regardless of token elevation.

fn scope(identifier: &str, user_sid: &str) -> String {
    format!("Local\\{identifier}-{user_sid}-gui")
}

#[cfg(target_os = "windows")]
mod windows {
    use super::scope;
    use crate::windows_service::native;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::thread::JoinHandle;
    use tauri::{
        plugin::{Builder, TauriPlugin},
        Manager, Resource, Runtime,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, WAIT_OBJECT_0},
        System::Threading::{
            CreateEventExW, CreateMutexExW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE,
            INFINITE, SYNCHRONIZATION_SYNCHRONIZE,
        },
    };

    struct Handle(isize);

    impl Handle {
        fn new(raw: windows_sys::Win32::Foundation::HANDLE) -> std::io::Result<Self> {
            if raw.is_null() {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(Self(raw as isize))
            }
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0 as _) };
        }
    }

    fn security(sid: &str, access: u32) -> std::io::Result<native::SecurityDescriptor> {
        // Grant only this user the required rights. A medium label also lets a
        // normal token signal an event created by the same user's elevated GUI.
        // The event carries no arguments and can only request window activation.
        native::SecurityDescriptor::from_sddl(&format!(
            "D:P(A;;0x{access:08x};;;{sid})S:(ML;;NW;;;ME)"
        ))
        .map_err(std::io::Error::other)
    }

    fn create_wake(key: &str, sid: &str) -> std::io::Result<Handle> {
        let access = SYNCHRONIZATION_SYNCHRONIZE | EVENT_MODIFY_STATE;
        let descriptor = security(sid, access)?;
        Handle::new(unsafe {
            CreateEventExW(
                &descriptor.attributes(),
                native::wide(format!("{key}-wake")).as_ptr(),
                0,
                access,
            )
        })
    }

    fn create_mutex(key: &str, sid: &str) -> std::io::Result<(Handle, bool)> {
        let descriptor = security(sid, SYNCHRONIZATION_SYNCHRONIZE)?;
        let raw = unsafe {
            CreateMutexExW(
                &descriptor.attributes(),
                native::wide(format!("{key}-lock")).as_ptr(),
                0,
                SYNCHRONIZATION_SYNCHRONIZE,
            )
        };
        let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        Ok((Handle::new(raw)?, existed))
    }

    struct Instance {
        _mutex: Handle,
        wake: Handle,
        stopping: Arc<AtomicBool>,
        worker: Option<JoinHandle<()>>,
    }

    impl Resource for Instance {}

    impl Drop for Instance {
        fn drop(&mut self) {
            self.stopping.store(true, Ordering::Release);
            unsafe { SetEvent(self.wake.0 as _) };
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            // Keep kernel objects alive until their listener has stopped.
        }
    }

    // Tests may inject a namespace SID; permissions always use the real token.
    // Production never accepts an identity from arguments or the environment.
    fn plugin<R: Runtime>(identity: Option<String>) -> TauriPlugin<R> {
        Builder::new("single-instance")
            .on_window_ready(|window| {
                if window.label() == "main" && native::current_elevation().unwrap_or(false) {
                    if let Ok(title) = window.title() {
                        let _ = window.set_title(&format!("{title}（管理员）"));
                    }
                }
            })
            .setup(move |app, _| {
                let sid = native::current_user_sid()?;
                let key = scope(
                    &app.config().identifier,
                    identity.as_deref().unwrap_or(&sid),
                );
                // Queue wakes even if another launch wins the mutex before its
                // listener is ready. Local names keep login sessions separate.
                let wake = create_wake(&key, &sid)?;
                let (mutex, existed) = create_mutex(&key, &sid)?;
                if existed {
                    if unsafe { SetEvent(wake.0 as _) } == 0 {
                        return Err(std::io::Error::last_os_error().into());
                    }
                    drop(mutex);
                    drop(wake);
                    app.cleanup_before_exit();
                    std::process::exit(0);
                }
                let stopping = Arc::new(AtomicBool::new(false));
                let stopped = Arc::clone(&stopping);
                let app_handle = app.clone();
                let event = wake.0;
                let worker = std::thread::Builder::new()
                    .name("gui-instance-wake".into())
                    .spawn(move || {
                        while unsafe { WaitForSingleObject(event as _, INFINITE) } == WAIT_OBJECT_0
                        {
                            if stopped.load(Ordering::Acquire) {
                                break;
                            }
                            let target = app_handle.clone();
                            let _ = app_handle.run_on_main_thread(move || {
                                let _ = crate::tray::show_main_window(&target);
                            });
                        }
                    })?;
                app.resources_table().add(Instance {
                    _mutex: mutex,
                    wake,
                    stopping,
                    worker: Some(worker),
                });
                Ok(())
            })
            .build()
    }

    pub(crate) fn init<R: Runtime>() -> TauriPlugin<R> {
        plugin(None)
    }

    #[cfg(test)]
    pub(crate) fn for_test<R: Runtime>(sid: &str) -> TauriPlugin<R> {
        plugin(Some(sid.into()))
    }

    #[cfg(test)]
    mod security_tests {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/gui_single_instance_security_tests.rs"
        ));
    }
}

#[cfg(all(test, target_os = "windows"))]
pub(crate) use windows::for_test;
#[cfg(target_os = "windows")]
pub(crate) use windows::init;

#[cfg(test)]
mod tests {
    use super::scope;

    #[test]
    fn instance_scope_is_per_user_and_session_without_a_privilege_partition() {
        let ordinary = scope("com.llremote", "S-1-5-21-1000");
        assert_eq!(ordinary, "Local\\com.llremote-S-1-5-21-1000-gui");
        assert_ne!(ordinary, scope("com.llremote", "S-1-5-21-1001"));
        assert_ne!(ordinary, scope("com.other", "S-1-5-21-1000"));
        assert!(ordinary.starts_with("Local\\"));
    }
}
