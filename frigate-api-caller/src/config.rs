#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrigateApiConfig {
    pub frigate_api_base_url: String,
    // e.g.: socks5://192.168.1.1:9000
    pub frigate_api_proxy: Option<String>,
}
