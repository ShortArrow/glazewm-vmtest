//! Reading what `glazewm-cli` prints, and preparing glazewm's configuration.
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

/// Returns glazewm's sample configuration with its startup and shutdown commands removed.
///
/// The sample starts and stops zebar, which a scenario that does not deploy zebar must not depend on.
pub fn config_without_startup_commands(sample: &str) -> String {
    sample
        .split_inclusive('\n')
        .map(|line| {
            let key = line.trim_start();
            if key.starts_with("startup_commands:") || key.starts_with("shutdown_commands:") {
                let indent = &line[..line.len() - key.len()];
                let name = &key[..key.find(':').unwrap_or(key.len())];
                let ending = if line.ends_with('\n') { "\n" } else { "" };
                format!("{indent}{name}: []{ending}")
            } else {
                line.to_string()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empties_only_the_startup_and_shutdown_commands() {
        let sample = "general:
  startup_commands: ['shell-exec zebar']

  shutdown_commands: ['shell-exec taskkill /IM zebar.exe /F']

  config_reload_commands: []
";

        assert_eq!(
            config_without_startup_commands(sample),
            "general:
  startup_commands: []

  shutdown_commands: []

  config_reload_commands: []
"
        );
    }

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
