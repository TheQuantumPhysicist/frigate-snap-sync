#![allow(dead_code)]

#[must_use]
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Review {
    pub id: String,
    pub camera: String,
    pub start_time: f64,
    pub end_time: Option<f64>,
    pub has_been_reviewed: bool,
    pub severity: String,
    pub thumb_path: String,
    pub data: Data,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Data {
    pub detections: Vec<String>,
    pub objects: Vec<String>,
    pub sub_labels: Vec<serde_json::Value>,
    pub zones: Vec<String>,
    pub audio: Vec<serde_json::Value>,
}
