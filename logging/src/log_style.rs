use thiserror::Error;

use crate::utils::{GetFromEnvError, get_from_env};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TextColoring {
    On,
    Off,
    Auto,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LogStyle {
    Text(TextColoring),
    Json,
}

impl LogStyle {
    pub fn parse(str: &str) -> Result<LogStyle, LogStyleParseError> {
        let str = str.to_lowercase();
        match str.as_str() {
            "json" => Ok(LogStyle::Json),
            "text" => Ok(LogStyle::Text(TextColoring::Auto)),
            "text-colored" => Ok(LogStyle::Text(TextColoring::On)),
            "text-uncolored" => Ok(LogStyle::Text(TextColoring::Off)),
            _ => Err(LogStyleParseError::UnrecognizedFormat(str)),
        }
    }
}

pub fn get_log_style_from_env(env_var_name: &str) -> Result<Option<LogStyle>, LogStyleParseError> {
    get_from_env(env_var_name)?
        .map(|val| LogStyle::parse(&val))
        .transpose()
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LogStyleParseError {
    #[error("Unrecognized format: {0}")]
    UnrecognizedFormat(String),
    #[error("Env var error: {0:?}")]
    GetFromEnvError(#[from] GetFromEnvError),
}

#[cfg(test)]
mod tests {
    use super::*;

    // Make the name verbose so that it doesn't conflict with env variables used by other
    // tests, if any.
    static TEST_ENV_VAR: &str = "LOG_STYLE_TEST_ENV_VAR";

    // Note: all checks are inside one test; if there were multiple tests, they would have
    // to use different names for the test env var, so that they wouldn't conflict if the tests
    // were run in parallel.
    #[test]
    fn parse_env_var() {
        // Basic tests
        {
            unsafe {
                std::env::set_var(TEST_ENV_VAR, "text");
                let result = get_log_style_from_env(TEST_ENV_VAR);
                assert_eq!(result, Ok(Some(LogStyle::Text(TextColoring::Auto))));

                std::env::set_var(TEST_ENV_VAR, "text-colored");
                let result = get_log_style_from_env(TEST_ENV_VAR);
                assert_eq!(result, Ok(Some(LogStyle::Text(TextColoring::On))));

                std::env::set_var(TEST_ENV_VAR, "text-uncolored");
                let result = get_log_style_from_env(TEST_ENV_VAR);
                assert_eq!(result, Ok(Some(LogStyle::Text(TextColoring::Off))));

                std::env::set_var(TEST_ENV_VAR, "json");
                let result = get_log_style_from_env(TEST_ENV_VAR);
                assert_eq!(result, Ok(Some(LogStyle::Json)));
            }
        }

        // Case-insensitivity tests
        {
            unsafe {
                std::env::set_var(TEST_ENV_VAR, "tEXt");
                let result = get_log_style_from_env(TEST_ENV_VAR);
                assert_eq!(result, Ok(Some(LogStyle::Text(TextColoring::Auto))));

                std::env::set_var(TEST_ENV_VAR, "tEXt-coLoRed");
                let result = get_log_style_from_env(TEST_ENV_VAR);
                assert_eq!(result, Ok(Some(LogStyle::Text(TextColoring::On))));

                std::env::set_var(TEST_ENV_VAR, "tEXt-uncoLoRed");
                let result = get_log_style_from_env(TEST_ENV_VAR);
                assert_eq!(result, Ok(Some(LogStyle::Text(TextColoring::Off))));

                std::env::set_var(TEST_ENV_VAR, "jSoN");
                let result = get_log_style_from_env(TEST_ENV_VAR);
                assert_eq!(result, Ok(Some(LogStyle::Json)));
            }
        }

        // Bad value test
        {
            let str = "foo";
            unsafe {
                std::env::set_var(TEST_ENV_VAR, str);
            }
            let result = get_log_style_from_env(TEST_ENV_VAR);
            assert_eq!(
                result,
                Err(LogStyleParseError::UnrecognizedFormat(str.to_owned()))
            );
        }

        // Missing value test
        {
            unsafe {
                std::env::remove_var(TEST_ENV_VAR);
            }
            let result = get_log_style_from_env(TEST_ENV_VAR);
            assert_eq!(result, Ok(None));
        }
    }
}
