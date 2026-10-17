use super::{GaiseLiveEvent, GaiseLiveInput};
use futures_util::Stream;
use std::collections::HashSet;
use std::pin::Pin;

pub type GaiseLiveEventStream = Pin<
    Box<
        dyn Stream<Item = Result<GaiseLiveEvent, Box<dyn std::error::Error + Send + Sync>>> + Send,
    >,
>;

pub struct GaiseLiveSession {
    pub tx: tokio::sync::mpsc::Sender<GaiseLiveInput>,
    pub rx: GaiseLiveEventStream,
}

/// Decides when a realtime session may ask the model to continue after
/// function calls (OpenAI Realtime and xAI `response.create`).
///
/// A response can issue several calls in parallel, and the provider wants
/// every `function_call_output` before the next `response.create` ("Do not
/// send `response.create` until all function call outputs have been
/// submitted"). The gate holds the continuation until the response that
/// issued the calls has finished **and** every call it issued has an output.
///
/// Transports feed it server events (`response_started`, `call_issued`,
/// `response_finished`) and client tool results (`output_submitted`); the
/// methods that can release the continuation return `true` exactly once per
/// batch, and the transport then sends `response.create`.
#[derive(Debug, Default, Clone)]
pub struct LiveToolCallGate {
    pending: HashSet<String>,
    response_active: bool,
    outputs_waiting: bool,
}

impl LiveToolCallGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// The server started generating a response (`response.created`).
    pub fn response_started(&mut self) {
        self.response_active = true;
    }

    /// The model issued a function call (`response.function_call_arguments.done`).
    pub fn call_issued(&mut self, call_id: &str) {
        self.pending.insert(call_id.to_string());
    }

    /// The response finished (`response.done`). Returns `true` when the
    /// continuation should be sent now: outputs arrived for every call
    /// while the response was still streaming.
    pub fn response_finished(&mut self) -> bool {
        self.response_active = false;
        self.release()
    }

    /// The client submitted a call's output. Returns `true` when the
    /// continuation should be sent now.
    pub fn output_submitted(&mut self, call_id: &str) -> bool {
        self.pending.remove(call_id);
        self.outputs_waiting = true;
        self.release()
    }

    /// Calls still waiting for an output.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    fn release(&mut self) -> bool {
        if self.outputs_waiting && self.pending.is_empty() && !self.response_active {
            self.outputs_waiting = false;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::LiveToolCallGate;

    #[test]
    fn single_call_after_the_response_finished_continues_immediately() {
        let mut gate = LiveToolCallGate::new();
        gate.response_started();
        gate.call_issued("a");
        assert!(!gate.response_finished(), "no output yet");
        assert!(gate.output_submitted("a"));
    }

    #[test]
    fn parallel_calls_wait_for_every_output() {
        let mut gate = LiveToolCallGate::new();
        gate.response_started();
        gate.call_issued("a");
        gate.call_issued("b");
        assert!(!gate.response_finished());
        assert!(!gate.output_submitted("a"), "b is still outstanding");
        assert_eq!(gate.pending(), 1);
        assert!(gate.output_submitted("b"));
        assert!(!gate.response_finished(), "released once per batch");
    }

    #[test]
    fn a_fast_output_waits_for_later_calls_in_the_same_response() {
        let mut gate = LiveToolCallGate::new();
        gate.response_started();
        gate.call_issued("a");
        // The client answers `a` before the server has announced `b`.
        assert!(!gate.output_submitted("a"), "response still streaming");
        gate.call_issued("b");
        assert!(!gate.response_finished(), "b has no output");
        assert!(gate.output_submitted("b"));
    }

    #[test]
    fn outputs_for_a_finished_response_release_on_response_done() {
        let mut gate = LiveToolCallGate::new();
        gate.response_started();
        gate.call_issued("a");
        assert!(!gate.output_submitted("a"));
        assert!(gate.response_finished(), "everything answered while streaming");
        assert!(!gate.response_finished());
    }

    #[test]
    fn unknown_call_ids_still_continue() {
        // Outputs for calls the gate never saw (e.g. replayed history) keep
        // the old behaviour of continuing once nothing else is pending.
        let mut gate = LiveToolCallGate::new();
        assert!(gate.output_submitted("from-history"));
    }
}
