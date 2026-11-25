//! Native-owned browser grant. Transport may request/observe, never approve.
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserBinding {
    pub native_incarnation: String,
    pub native_generation: String,
    pub native_connection: String,
    pub native_pid: u32,
    pub native_uid: u32,
    pub project: String,
    pub scene_id: String,
    pub session_token: String,
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
    #[cfg(test)]
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
            binding: Some(BrowserBinding {
                native_incarnation: "native".into(),
                native_generation: "generation".into(),
                native_connection: "connection".into(),
                native_pid: 1,
                native_uid: 2,
                project: "/saved/project".into(),
                scene_id: "scene".into(),
                session_token: "session".into(),
                attachment: "a".repeat(64),
                peer_pid: 3,
                origin: "http://127.0.0.1:1234".into(),
            }),
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
        for field in ["origin", "peer", "session", "project", "attachment"] {
            let mut replaced = request.clone();
            let binding = replaced.binding.as_mut().unwrap();
            match field {
                "origin" => binding.origin.push_str("/other"),
                "peer" => binding.peer_pid += 1,
                "session" => binding.session_token.push_str("-old"),
                "project" => binding.project.push_str("-other"),
                _ => binding.attachment.push('a'),
            }
            assert_eq!(pairing.state(&replaced), "denied", "{field}");
        }
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
