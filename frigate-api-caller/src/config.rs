#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrigateApiConfig {
    pub frigate_api_base_url: String,
    // e.g.: socks5://192.168.1.1:9000
    pub frigate_api_proxy: Option<String>,
    pub frigate_api_auth: Option<FrigateApiAuthConfig>,
    // Uptime of Frigate to wait for, after which uploads can happen
    pub delay_after_startup: std::time::Duration,
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrigateApiAuthConfig {
    pub username: String,
    pub password: String,
}
