const MAX_APP_AGENT_REQUESTS: usize = 16;
static ACTIVE_APP_AGENT_REQUESTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

pub(crate) struct AgentRequestPermit;

pub(crate) fn admit_agent_request() -> Result<AgentRequestPermit, String> {
    let mut active = ACTIVE_APP_AGENT_REQUESTS.load(Ordering::Acquire);
    loop {
        if active >= MAX_APP_AGENT_REQUESTS {
            return Err("agent_request_busy".into());
        }
        match ACTIVE_APP_AGENT_REQUESTS.compare_exchange_weak(
            active,
            active + 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return Ok(AgentRequestPermit),
            Err(observed) => active = observed,
        }
    }
}

impl Drop for AgentRequestPermit {
    fn drop(&mut self) {
        ACTIVE_APP_AGENT_REQUESTS.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
#[path = "../../tests/support/agent_request_limit_tests.rs"]
mod request_limit_tests;
