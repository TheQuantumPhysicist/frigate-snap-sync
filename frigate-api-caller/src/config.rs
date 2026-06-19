use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrigateApiConfig {
    pub frigate_api_base_url: String,
    // e.g.: socks5://192.168.1.1:9000
    pub frigate_api_proxy: Option<String>,
    // Credentials used to authenticate against Frigate's API, if it requires login.
    pub frigate_api_auth: Option<FrigateApiAuthConfig>,
    // Uptime of Frigate to wait for, after which uploads can happen
    pub delay_after_startup: std::time::Duration,
}

// - Resolved credentials handed to the API client to log in.
// - The password is zeroized on drop so it does not linger in freed memory.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct FrigateApiAuthConfig {
    pub username: String,
    pub password: String,
}

// - Custom Debug that never prints the password.
// - Zeroize protects freed memory; this protects logs and error messages.
impl fmt::Debug for FrigateApiAuthConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrigateApiAuthConfig")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}
