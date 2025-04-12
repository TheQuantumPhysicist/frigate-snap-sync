pub fn on_off_from_bytes(value: Vec<u8>) -> Option<bool> {
    let value = String::from_utf8(value).ok()?;
    let value = value.trim();
    if value == "ON" {
        Some(true)
    } else if value == "OFF" {
        Some(false)
    } else {
        None
    }
}
