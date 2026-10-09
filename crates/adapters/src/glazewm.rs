//! Reading what `glazewm-cli` prints.
//!
//! The JSON is read without glazewm's own types, so that SUT commits with a different IPC schema can still be observed.

/// Returns the number of monitors in the output of `glazewm-cli query monitors`.
pub fn monitor_count(stdout: &str) -> Result<usize, String> {
    let answer: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|error| format!("glazewm-cli printed something other than JSON: {error}"))?;
    if answer["success"] != serde_json::Value::Bool(true) {
        let error = answer["error"].as_str().unwrap_or("no error message");
        return Err(format!("glazewm answered with an error: {error}"));
    }
    answer["data"]["monitors"]
        .as_array()
        .map(Vec::len)
        .ok_or_else(|| "glazewm's answer has no data.monitors array".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_the_monitors_in_a_successful_answer() {
        let stdout = r#"{"clientMessage":"query monitors","data":{"monitors":[{"id":"a"},{"id":"b"}]},"error":null,"success":true}"#;

        assert_eq!(monitor_count(stdout), Ok(2));
    }

    #[test]
    fn reports_the_error_of_an_unsuccessful_answer() {
        let stdout = r#"{"clientMessage":"query monitors","data":null,"error":"WM is not running","success":false}"#;

        assert_eq!(
            monitor_count(stdout),
            Err("glazewm answered with an error: WM is not running".into())
        );
    }

    #[test]
    fn ignores_fields_it_does_not_know() {
        let stdout = r#"{"clientMessage":"query monitors","data":{"monitors":[{"id":"a","newField":1}],"extra":true},"error":null,"success":true,"version":"9"}"#;

        assert_eq!(monitor_count(stdout), Ok(1));
    }
}
