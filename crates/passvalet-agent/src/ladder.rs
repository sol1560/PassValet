//! Escalate to a stronger model after repeated failures on the same step.

#[derive(Debug, Clone)]
pub struct ModelLadder {
    models: Vec<String>,
    index: usize,
    consecutive_failures: u32,
    pub failures_before_escalation: u32,
    pub escalations: u32,
}

impl ModelLadder {
    pub fn new(models: Vec<String>) -> Self {
        assert!(!models.is_empty(), "ladder needs at least one model");
        ModelLadder {
            models,
            index: 0,
            consecutive_failures: 0,
            failures_before_escalation: 3,
            escalations: 0,
        }
    }

    pub fn current(&self) -> &str {
        &self.models[self.index]
    }

    pub fn is_top(&self) -> bool {
        self.index + 1 >= self.models.len()
    }

    pub fn note_success(&mut self) {
        self.consecutive_failures = 0;
    }

    /// Record a failed step. Returns `Some(new_model)` if we escalated.
    pub fn note_failure(&mut self) -> Option<String> {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= self.failures_before_escalation && !self.is_top() {
            self.index += 1;
            self.consecutive_failures = 0;
            self.escalations += 1;
            return Some(self.current().to_string());
        }
        None
    }

    /// Provider-level error (5xx, rate limit): escalate immediately if possible.
    pub fn force_escalate(&mut self) -> Option<String> {
        if self.is_top() {
            return None;
        }
        self.index += 1;
        self.consecutive_failures = 0;
        self.escalations += 1;
        Some(self.current().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalates_after_three() {
        let mut l = ModelLadder::new(vec!["a".into(), "b".into()]);
        assert!(l.note_failure().is_none());
        assert!(l.note_failure().is_none());
        assert_eq!(l.note_failure().as_deref(), Some("b"));
        assert!(l.note_failure().is_none());
        assert!(l.is_top());
    }
}
