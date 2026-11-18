//! Per-supervisor test cut; no global hook and no production mutex held while parked.
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum RegistrationCut {
    BeforeAdopt,
    AfterAdoptBeforeStart,
}

pub struct RegistrationRaceGate {
    pub cut: RegistrationCut,
    target: crate::domain::native::NativeSessionKey,
    reached: AtomicBool,
    released: Mutex<bool>,
    changed: Condvar,
}

impl RegistrationRaceGate {
    pub fn new(cut: RegistrationCut, target: crate::domain::native::NativeSessionKey) -> Arc<Self> {
        Arc::new(Self {
            cut,
            target,
            reached: AtomicBool::new(false),
            released: Mutex::new(false),
            changed: Condvar::new(),
        })
    }

    pub fn reached(&self) -> bool {
        self.reached.load(Ordering::Acquire)
    }

    pub fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.changed.notify_all();
    }

    pub(super) fn park(&self) {
        self.reached.store(true, Ordering::Release);
        let (released, _) = self
            .changed
            .wait_timeout_while(
                self.released.lock().unwrap(),
                Duration::from_secs(15),
                |released| !*released,
            )
            .unwrap();
        let completed = *released;
        drop(released);
        assert!(completed, "owned registration race gate timed out");
    }
}

pub struct ReleaseRegistrationGate(pub Arc<RegistrationRaceGate>);
impl Drop for ReleaseRegistrationGate {
    fn drop(&mut self) {
        self.0.release();
    }
}

impl super::ViewTaskSupervisor {
    pub(crate) fn capacity_install_registration_gate(&self, gate: Arc<RegistrationRaceGate>) {
        let mut current = self.state.capacity_registration_gate.lock().unwrap();
        assert!(current.is_none(), "only one owned race cut per supervisor");
        *current = Some(gate);
    }

    pub(super) fn capacity_registration_cut(
        &self,
        cut: RegistrationCut,
        target: &crate::domain::native::NativeSessionKey,
    ) {
        let gate = {
            let mut gate = self.state.capacity_registration_gate.lock().unwrap();
            if gate
                .as_ref()
                .is_some_and(|gate| gate.cut == cut && &gate.target == target)
            {
                gate.take()
            } else {
                None
            }
        };
        if let Some(gate) = gate {
            gate.park();
        }
    }
}
