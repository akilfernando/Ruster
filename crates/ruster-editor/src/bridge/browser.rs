//! Native-owned browser grant. Transport may request/observe, never approve.
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserBinding {
    pub native: String,
    pub attachment: String,
    pub peer_pid: u32,
    pub origin: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserRequest {
    pub id: String,
    pub context: String,
    pub binding: Option<BrowserBinding>,
}

pub struct BrowserPairing {
    request: BrowserRequest,
    deadline: Instant,
    approved: bool,
    denied: bool,
}
impl BrowserPairing {
    pub(crate) fn new(request: BrowserRequest) -> Self {
        Self {
            request,
            deadline: Instant::now() + Duration::from_secs(120),
            approved: false,
            denied: false,
        }
    }
    pub fn request(&self) -> &BrowserRequest {
        &self.request
    }
    pub fn state(&self, request: &BrowserRequest) -> &'static str {
        if self.request != *request || self.denied || Instant::now() >= self.deadline {
            "denied"
        } else if self.approved {
            "approved"
        } else {
            "pending"
        }
    }
    pub fn approve(&mut self, request: &BrowserRequest) -> bool {
        if self.state(request) != "pending" {
            return false;
        }
        self.approved = true;
        self.deadline = Instant::now() + Duration::from_secs(1800);
        true
    }
    pub fn deny(&mut self) {
        self.denied = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_context_one_shot_denial_and_expiry() {
        let request = BrowserRequest {
            id: "a".repeat(64),
            context: "exact scene/session/origin".into(),
            binding: None,
        };
        let mut pairing = BrowserPairing::new(request.clone());
        let wrong = BrowserRequest {
            context: "other session".into(),
            ..request.clone()
        };
        assert_eq!(pairing.state(&request), "pending");
        assert!(!pairing.approve(&wrong));
        assert!(pairing.approve(&request));
        assert!(!pairing.approve(&request));
        assert_eq!(pairing.state(&wrong), "denied");
        pairing.deadline = Instant::now();
        assert_eq!(pairing.state(&request), "denied");
        assert!(!pairing.approve(&request));
        let mut denied = BrowserPairing::new(request.clone());
        denied.deny();
        assert!(!denied.approve(&request));
        assert_eq!(denied.state(&request), "denied");
        let mut expired = BrowserPairing::new(request.clone());
        expired.deadline = Instant::now();
        assert!(!expired.approve(&request));
    }
}
