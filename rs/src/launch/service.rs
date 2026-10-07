//! SMAppService behind a trait, so the engine's registration logic runs against a fake in tests.

use objc2::rc::Retained;
use objc2_foundation::{NSError, NSString};
use objc2_service_management::{SMAppService, SMAppServiceStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    NotRegistered,
    Enabled,
    RequiresApproval,
    NotFound,
    Unknown,
}

pub trait AgentService {
    fn status(&self) -> AgentStatus;
    fn register(&self) -> Result<(), String>;
    fn unregister(&self) -> Result<(), String>;
}

/// One SMAppService: the bundle's launchd agent, or the app itself as a login item.
pub struct SmApp(Retained<SMAppService>);

impl SmApp {
    /// `SMAppService.agent(plistName:)`.
    pub fn agent(plist_name: &str) -> Self {
        // SAFETY: plain class constructor.
        Self(unsafe { SMAppService::agentServiceWithPlistName(&NSString::from_str(plist_name)) })
    }

    /// `SMAppService.mainApp`, which versions up to 0.3.2 registered as a login item.
    pub fn main_app() -> Self {
        // SAFETY: plain class accessor.
        Self(unsafe { SMAppService::mainAppService() })
    }
}

fn describe(e: Retained<NSError>) -> String {
    e.localizedDescription().to_string()
}

impl AgentService for SmApp {
    fn status(&self) -> AgentStatus {
        // SAFETY: plain property read.
        match unsafe { self.0.status() } {
            SMAppServiceStatus::NotRegistered => AgentStatus::NotRegistered,
            SMAppServiceStatus::Enabled => AgentStatus::Enabled,
            SMAppServiceStatus::RequiresApproval => AgentStatus::RequiresApproval,
            SMAppServiceStatus::NotFound => AgentStatus::NotFound,
            _ => AgentStatus::Unknown,
        }
    }
    fn register(&self) -> Result<(), String> {
        // SAFETY: errors come back as NSError, not exceptions.
        unsafe { self.0.registerAndReturnError() }.map_err(describe)
    }
    fn unregister(&self) -> Result<(), String> {
        // SAFETY: as above.
        unsafe { self.0.unregisterAndReturnError() }.map_err(describe)
    }
}
