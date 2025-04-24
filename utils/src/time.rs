#[must_use]
pub fn now_unix_timestamp_f64() -> f64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards");

    now.as_secs_f64()
}
