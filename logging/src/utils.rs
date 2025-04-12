use std::{borrow::Cow, ffi::OsString};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GetFromEnvError {
    #[error("Env var {var_name}'s contents are not valid unicode: {data:?}")]
    NotUnicode { var_name: String, data: OsString },
}

pub fn get_from_env(var_name: &str) -> Result<Option<String>, GetFromEnvError> {
    match std::env::var(var_name) {
        Ok(str) => Ok(Some(str)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(data)) => Err(GetFromEnvError::NotUnicode {
            var_name: var_name.to_owned(),
            data,
        }),
    }
}

pub enum ValueOrEnvVar<T> {
    Value(T),
    EnvVar(Cow<'static, str>),
}
