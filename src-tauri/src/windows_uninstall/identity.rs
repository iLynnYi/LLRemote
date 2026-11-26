//! An installation's owner and the operator authorizing its removal are distinct.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Installation {
    pub user_sid: Option<String>,
}

pub(super) struct Plan {
    pub user_sid: String,
    pub machine: bool,
    pub needs_elevation: bool,
}

impl Plan {
    pub fn service_needs_elevation(&self, associated_service: bool, elevated: bool) -> bool {
        associated_service && !elevated
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum CleanupContext {
    CurrentUser,
    OwnerSession,
    Offline,
}

impl CleanupContext {
    pub fn select(
        owner: &str,
        operator: &str,
        logged_on: bool,
        _delete_data: bool,
    ) -> Result<Self, String> {
        if owner == operator {
            return Ok(Self::CurrentUser);
        }
        if logged_on {
            return Ok(Self::OwnerSession);
        }
        Ok(Self::Offline)
    }
}

pub(super) fn valid_user_sid(sid: &str) -> bool {
    // Interactive local/domain and Entra accounts. Never accept SYSTEM or a
    // group as the personal-data target, or arbitrary command-line characters.
    (sid.starts_with("S-1-5-21-") || sid.starts_with("S-1-12-1-"))
        && sid.split('-').count() == 8
        && sid.split('-').skip(1).all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && part.parse::<u32>().is_ok()
        })
}

impl Installation {
    pub fn user(sid: String) -> Self {
        Self {
            user_sid: Some(sid),
        }
    }

    pub fn machine() -> Self {
        Self { user_sid: None }
    }

    pub fn encode(&self) -> String {
        match &self.user_sid {
            Some(sid) => format!("1\nuser\n{sid}\n"),
            None => "1\nall\n\n".into(),
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        let lines: Vec<_> = value.lines().collect();
        match lines.as_slice() {
            ["1", "user", sid] if valid_user_sid(sid) => Ok(Self::user((*sid).into())),
            ["1", "all", ""] => Ok(Self::machine()),
            _ => Err("windows_uninstall_owner_unknown".into()),
        }
    }

    pub fn plan(
        &self,
        operator: &str,
        elevated: bool,
        requested: Option<&str>,
    ) -> Result<Plan, String> {
        let sid = self
            .user_sid
            .as_deref()
            .unwrap_or(requested.unwrap_or(operator));
        if !valid_user_sid(sid) || self.user_sid.is_some() && requested.is_some_and(|v| v != sid) {
            return Err("windows_uninstall_owner_mismatch".into());
        }
        Ok(Plan {
            user_sid: sid.into(),
            machine: self.user_sid.is_none(),
            needs_elevation: !elevated && (self.user_sid.is_none() || sid != operator),
        })
    }
}

#[cfg(test)]
#[path = "../../tests/support/windows_uninstall_identity_tests.rs"]
mod tests;
