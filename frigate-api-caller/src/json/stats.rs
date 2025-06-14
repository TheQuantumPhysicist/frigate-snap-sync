use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct Stats {
    pub cameras: HashMap<String, CameraStats>,
    pub detectors: HashMap<String, DetectorStats>,

    pub detection_fps: f64,

    /// Only present if GPU stats were collected
    pub gpu_usages: Option<HashMap<String, GpuUsage>>,

    pub cpu_usages: Option<HashMap<String, CpuUsage>>,

    pub service: ServiceInfo,
    pub processes: HashMap<String, ProcessInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CameraStats {
    pub camera_fps: f64,
    pub process_fps: f64,
    pub skipped_fps: f64,
    pub detection_fps: f64,
    pub detection_enabled: bool,

    /// May be None if process wasn't started
    #[serde(default)]
    pub pid: Option<u32>,

    /// May be None if capture process wasn't started
    #[serde(default)]
    pub capture_pid: Option<u32>,

    /// May be None if `FFmpeg` wasn't running
    #[serde(default)]
    pub ffmpeg_pid: Option<u32>,

    #[serde(default)]
    pub audio_rms: Option<f64>,

    #[serde(rename = "audio_dBFS", default)]
    pub audio_dbfs: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DetectorStats {
    pub inference_speed: f64,
    pub detection_start: f64,

    /// May be None if the detect process isn't running
    #[serde(default)]
    pub pid: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GpuUsage {
    pub gpu: String,
    pub mem: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CpuUsage {
    pub cpu: String,

    /// Only present if the metric exists
    #[serde(default)]
    pub cpu_average: Option<String>,

    pub mem: String,

    /// Only present if cmd-line was captured
    #[serde(default)]
    pub cmdline: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub uptime: u64,
    pub version: String,

    /// May be None if version check is disabled
    #[serde(default)]
    pub latest_version: Option<String>,

    /// Always present, but individual `StorageInfo` fields may be None
    #[serde(default)]
    pub storage: HashMap<String, StorageInfo>,

    #[serde(default)]
    pub temperatures: HashMap<String, f64>,

    pub last_updated: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StorageInfo {
    #[serde(default)]
    pub total: Option<f64>,
    #[serde(default)]
    pub used: Option<f64>,
    #[serde(default)]
    pub free: Option<f64>,
    #[serde(default)]
    pub mount_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
}

impl Stats {
    #[must_use]
    pub fn uptime_duration(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.service.uptime)
    }
}

pub trait StatsProps {
    fn uptime(&self) -> std::time::Duration;
}

impl StatsProps for Stats {
    fn uptime(&self) -> std::time::Duration {
        self.uptime_duration()
    }
}
