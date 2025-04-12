#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MqttHandlerConfig {
    pub mqtt_frigate_topic_prefix: String,
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_keep_alive_seconds: u64,
    pub mqtt_username: Option<String>,
    pub mqtt_password: Option<String>,
    pub mqtt_client_id: String,
}
