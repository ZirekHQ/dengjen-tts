use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A cheaply-cloneable flag used to request early termination of an in-flight
/// streaming synthesis from another thread. Clones observe the same state.
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_token_is_not_cancelled() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn cancel_marks_token_cancelled() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn clones_share_cancellation_state() {
        let token = CancellationToken::new();
        let clone = token.clone();
        clone.cancel();
        assert!(token.is_cancelled(), "cancelling a clone must be visible on the original");
    }

    #[test]
    fn default_token_is_not_cancelled() {
        assert!(!CancellationToken::default().is_cancelled());
    }
}
