use std::collections::HashMap;

const DEFAULT_CAMERA_RECORDINGS_STATE: bool = false;
const DEFAULT_CAMERA_SNAPSHOTS_STATE: bool = false;

#[derive(Debug, Clone, Default)]
pub struct CamerasState {
    cameras_recordings_state: HashMap<String, bool>,
    cameras_snapshots_state: HashMap<String, bool>,
}

impl CamerasState {
    pub fn camera_recordings_state(&self, camera_name: impl AsRef<str>) -> bool {
        self.cameras_recordings_state
            .get(camera_name.as_ref())
            .copied()
            .unwrap_or(DEFAULT_CAMERA_RECORDINGS_STATE)
    }

    pub fn camera_snapshots_state(&self, camera_name: impl AsRef<str>) -> bool {
        self.cameras_snapshots_state
            .get(camera_name.as_ref())
            .copied()
            .unwrap_or(DEFAULT_CAMERA_SNAPSHOTS_STATE)
    }

    pub fn update_recordings_state(&mut self, camera_name: impl Into<String>, value: bool) {
        let camera_name = camera_name.into();
        tracing::debug!("Updating recordings state of camera `{camera_name}` to `{value}`");
        self.cameras_recordings_state.insert(camera_name, value);
    }

    pub fn update_snapshots_state(&mut self, camera_name: impl Into<String>, value: bool) {
        let camera_name = camera_name.into();
        tracing::debug!("Updating snapshots state of camera `{camera_name}` to `{value}`");
        self.cameras_snapshots_state.insert(camera_name, value);
    }

    pub fn recordings_state(&self) -> &HashMap<String, bool> {
        &self.cameras_recordings_state
    }

    pub fn snapshots_state(&self) -> &HashMap<String, bool> {
        &self.cameras_snapshots_state
    }
}
