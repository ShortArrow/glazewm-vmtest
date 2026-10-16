//! How much one request may ask of the guest daemon, and how long the daemon waits for what it starts,
//! so that no request keeps it busy without end.
//!
//! The limits come from a file the runner delivers beside the daemon, never from a request, so that
//! nothing reaching the daemon's port can raise them.

use std::time::Duration;

use serde::Deserialize;

use crate::test_windows::Batch;

/// The most one request may ask for, and the longest the daemon waits for what it starts.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub max_test_windows_per_request: u32,
    pub max_test_window_span_ms: u64,
    /// How long a started test-window application may take to connect back.
    pub test_window_connect_limit_ms: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_test_windows_per_request: 500,
            max_test_window_span_ms: 60_000,
            test_window_connect_limit_ms: 3_000,
        }
    }
}

impl Limits {
    /// Reads limits from the JSON of a configuration file; a field it leaves out keeps its default.
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|error| format!("invalid daemon configuration: {error}"))
    }

    /// Says why `batch` asks for more than these limits allow, if it does.
    pub fn check(&self, batch: &Batch) -> Result<(), String> {
        if batch.count > self.max_test_windows_per_request {
            return Err(format!(
                "{} test windows exceed the {} one request may open",
                batch.count, self.max_test_windows_per_request
            ));
        }
        let longest = Duration::from_millis(self.max_test_window_span_ms);
        match batch.within {
            Some(span) if span > longest => Err(format!(
                "a span of {span:?} exceeds the {longest:?} one request may take"
            )),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(count: u32, within: Option<Duration>) -> Batch {
        Batch {
            count,
            within,
            lifetime_ms: None,
        }
    }

    #[test]
    fn allows_up_to_500_windows_within_60_seconds_by_default() {
        let limits = Limits::default();

        assert_eq!(
            limits.check(&batch(500, Some(Duration::from_secs(60)))),
            Ok(())
        );
        assert!(limits.check(&batch(501, None)).is_err());
        assert!(
            limits
                .check(&batch(1, Some(Duration::from_millis(60_001))))
                .is_err()
        );
    }

    #[test]
    fn waits_3_seconds_for_a_test_window_application_to_connect_by_default() {
        assert_eq!(Limits::default().test_window_connect_limit_ms, 3_000);
    }

    #[test]
    fn reads_the_limits_a_configuration_file_sets_and_defaults_the_rest() {
        let limits = Limits::from_json(r#"{"max_test_windows_per_request": 2000}"#).unwrap();

        assert_eq!(
            limits,
            Limits {
                max_test_windows_per_request: 2000,
                ..Limits::default()
            }
        );
    }

    #[test]
    fn rejects_a_configuration_with_a_field_it_does_not_know() {
        assert!(Limits::from_json(r#"{"max_windows": 10}"#).is_err());
    }
}
