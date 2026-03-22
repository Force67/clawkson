/// Loop detection: detect and stop runaway tool-calling cycles.
///
/// Tracks (tool_name, argument_hash) pairs in a sliding window.
/// After 3 identical calls in 60s: inject warning.
/// After 5 identical calls in 60s: abort generation.
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

/// A tool call fingerprint for loop detection.
#[derive(Debug, Clone, Eq, PartialEq)]
struct ToolFingerprint {
    tool_name: String,
    arg_hash: u64,
}

impl ToolFingerprint {
    fn new(tool_name: &str, args: &str) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        args.hash(&mut hasher);
        Self {
            tool_name: tool_name.to_string(),
            arg_hash: hasher.finish(),
        }
    }
}

/// Result of checking a tool call against the loop detector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopCheckResult {
    /// No loop detected — proceed normally.
    Ok,
    /// Warning threshold reached — inject a warning message.
    Warning {
        tool_name: String,
        count: usize,
    },
    /// Abort threshold reached — stop generation.
    Abort {
        tool_name: String,
        count: usize,
    },
}

/// Per-conversation loop detector.
pub struct LoopDetector {
    /// Sliding window of recent tool calls.
    window: Vec<(ToolFingerprint, Instant)>,
    /// Count of each fingerprint within the window.
    counts: HashMap<(String, u64), usize>,
    /// Window duration (default 60s).
    window_duration: Duration,
    /// Number of identical calls before warning.
    warn_threshold: usize,
    /// Number of identical calls before abort.
    abort_threshold: usize,
}

impl LoopDetector {
    pub fn new() -> Self {
        Self {
            window: Vec::new(),
            counts: HashMap::new(),
            window_duration: Duration::from_secs(60),
            warn_threshold: 3,
            abort_threshold: 5,
        }
    }

    /// Record a tool call and check for loops.
    pub fn check(&mut self, tool_name: &str, args: &str) -> LoopCheckResult {
        let now = Instant::now();
        let fp = ToolFingerprint::new(tool_name, args);

        // Expire old entries outside the window
        self.expire(now);

        // Record the new call
        let key = (fp.tool_name.clone(), fp.arg_hash);
        *self.counts.entry(key.clone()).or_insert(0) += 1;
        self.window.push((fp, now));

        let count = self.counts[&key];

        if count >= self.abort_threshold {
            LoopCheckResult::Abort {
                tool_name: key.0,
                count,
            }
        } else if count >= self.warn_threshold {
            LoopCheckResult::Warning {
                tool_name: key.0,
                count,
            }
        } else {
            LoopCheckResult::Ok
        }
    }

    /// Reset the detector (e.g. when a new user message arrives).
    pub fn reset(&mut self) {
        self.window.clear();
        self.counts.clear();
    }

    fn expire(&mut self, now: Instant) {
        let cutoff = now - self.window_duration;
        let mut i = 0;
        while i < self.window.len() {
            if self.window[i].1 < cutoff {
                let (fp, _) = self.window.remove(i);
                let key = (fp.tool_name, fp.arg_hash);
                if let Some(count) = self.counts.get_mut(&key) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.counts.remove(&key);
                    }
                }
            } else {
                i += 1;
            }
        }
    }
}

impl Default for LoopDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_loop() {
        let mut detector = LoopDetector::new();
        assert_eq!(detector.check("tool_a", r#"{"query":"hello"}"#), LoopCheckResult::Ok);
        assert_eq!(detector.check("tool_b", r#"{"query":"world"}"#), LoopCheckResult::Ok);
    }

    #[test]
    fn test_warning_threshold() {
        let mut detector = LoopDetector::new();
        let args = r#"{"query":"same"}"#;
        assert_eq!(detector.check("tool_a", args), LoopCheckResult::Ok);
        assert_eq!(detector.check("tool_a", args), LoopCheckResult::Ok);
        match detector.check("tool_a", args) {
            LoopCheckResult::Warning { count, .. } => assert_eq!(count, 3),
            other => panic!("expected Warning, got {:?}", other),
        }
    }

    #[test]
    fn test_abort_threshold() {
        let mut detector = LoopDetector::new();
        let args = r#"{"query":"same"}"#;
        for _ in 0..4 {
            detector.check("tool_a", args);
        }
        match detector.check("tool_a", args) {
            LoopCheckResult::Abort { count, .. } => assert_eq!(count, 5),
            other => panic!("expected Abort, got {:?}", other),
        }
    }

    #[test]
    fn test_different_args_no_loop() {
        let mut detector = LoopDetector::new();
        for i in 0..10 {
            assert_eq!(detector.check("tool_a", &format!(r#"{{"query":"q{}"}}"#, i)), LoopCheckResult::Ok);
        }
    }

    #[test]
    fn test_reset() {
        let mut detector = LoopDetector::new();
        let args = r#"{"query":"same"}"#;
        detector.check("tool_a", args);
        detector.check("tool_a", args);
        detector.reset();
        assert_eq!(detector.check("tool_a", args), LoopCheckResult::Ok);
    }
}
