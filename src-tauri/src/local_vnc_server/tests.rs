#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_worker_keeps_refresh_mode_interval_when_agent_sidecar_is_online() {
        let connections = LocalVncServerConnectionRegistry::new(None);

        assert_eq!(
            capture_worker_refresh_interval(LocalVncServerRefreshMode::Balanced),
            refresh_interval(LocalVncServerRefreshMode::Balanced)
        );

        let connections = Arc::new(connections);
        let _guard = connections.register(
            LocalVncServerConnectionKind::Rfb,
            "127.0.0.1:6903".parse().ok(),
            Arc::new(|| {}),
        );

        assert_eq!(
            capture_worker_refresh_interval(LocalVncServerRefreshMode::Balanced),
            refresh_interval(LocalVncServerRefreshMode::Balanced)
        );
    }
}
