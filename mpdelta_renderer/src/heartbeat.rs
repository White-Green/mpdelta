use std::sync::atomic::AtomicBool;
use std::sync::{atomic, Arc};

pub fn heartbeat() -> (HeartbeatController, HeartbeatMonitor) {
    let live = Arc::new(AtomicBool::new(true));
    (HeartbeatController { live: Arc::clone(&live) }, HeartbeatMonitor { live })
}

pub struct HeartbeatController {
    live: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct HeartbeatMonitor {
    live: Arc<AtomicBool>,
}

impl Drop for HeartbeatController {
    fn drop(&mut self) {
        self.live.store(false, atomic::Ordering::Release);
    }
}

impl HeartbeatMonitor {
    pub fn is_live(&self) -> bool {
        self.live.load(atomic::Ordering::Acquire)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat() {
        let (controller, monitor) = heartbeat();
        assert!(monitor.is_live());
        drop(controller);
        assert!(!monitor.is_live());
    }
}
