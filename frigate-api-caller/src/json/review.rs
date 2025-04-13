#![allow(dead_code)]

#[must_use]
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Review {
    id: String,
    camera: String,
    start_time: f64,
    end_time: Option<f64>,
    has_been_reviewed: bool,
    severity: String,
    thumb_path: String,
    data: Data,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Data {
    detections: Vec<String>,
    objects: Vec<String>,
    sub_labels: Vec<String>, // Assuming this is a vector of strings
    zones: Vec<String>,
    audio: Vec<String>,
}
