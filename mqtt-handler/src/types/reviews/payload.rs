#![allow(dead_code)] // Not everything is needed, but we want to parse the whole json as future-proofing

#[derive(Debug, serde::Deserialize, Clone)]
pub struct ReviewsPayload {
    #[serde(rename = "type")]
    pub type_field: TypeField,
    pub before: BeforeAfterField,
    pub after: BeforeAfterField,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum TypeField {
    New,
    Update,
    End,
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct BeforeAfterField {
    pub id: String,
    pub camera: String,
    pub start_time: f64,
    pub end_time: Option<f64>,
    pub severity: String,
    pub thumb_path: String,
    pub data: ReviewData,
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct ReviewData {
    detections: Vec<String>, // Assuming these are detection IDs
    objects: Vec<String>,    // Array of object labels (e.g., "person")
    sub_labels: Vec<serde_json::Value>,
    zones: Vec<String>, // Array of zone names (e.g., "full_frame")
    audio: Vec<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        {
            let new_sample_data = r#"{"type": "new", "before": {"id": "1745534741.333822-vsz5s4", "camera": "CameraLabel", "start_time": 1745534741.333822, "end_time": null, "severity": "alert", "thumb_path": "/media/frigate/clips/review/thumb-CameraLabel-1745534741.333822-vsz5s4.webp", "data": {"detections": ["1744534706.323662-abcdefg"], "objects": ["person"], "sub_labels": [], "zones": ["full_frame"], "audio": []}}, "after": {"id": "1745534741.333822-vsz5s4", "camera": "CameraLabel", "start_time": 1745534741.333822, "end_time": null, "severity": "alert", "thumb_path": "/media/frigate/clips/review/thumb-CameraLabel-1745534741.333822-vsz5s4.webp", "data": {"detections": ["1744534706.323662-abcdefg"], "objects": ["person"], "sub_labels": [], "zones": ["full_frame"], "audio": []}}}"#;
            let new_data = serde_json::from_str::<ReviewsPayload>(&new_sample_data).unwrap();
            assert_eq!(new_data.type_field, TypeField::New);
            assert_eq!(new_data.after.camera, "CameraLabel");
            assert_eq!(new_data.before.camera, "CameraLabel");
            assert_eq!(new_data.before.id, "1745534741.333822-vsz5s4");
            assert_eq!(new_data.after.id, "1745534741.333822-vsz5s4");
            assert_eq!(new_data.before.start_time, 1745534741.333822);
            assert_eq!(new_data.after.start_time, 1745534741.333822);
            assert_eq!(new_data.before.end_time, None);
            assert_eq!(new_data.after.end_time, None);
            assert_eq!(new_data.before.severity, "alert");
            assert_eq!(new_data.after.severity, "alert");
            assert_eq!(
                new_data.before.thumb_path,
                "/media/frigate/clips/review/thumb-CameraLabel-1745534741.333822-vsz5s4.webp"
            );
            assert_eq!(
                new_data.after.thumb_path,
                "/media/frigate/clips/review/thumb-CameraLabel-1745534741.333822-vsz5s4.webp"
            );
            assert_eq!(
                new_data.before.data.detections,
                ["1744534706.323662-abcdefg"]
            );
            assert_eq!(
                new_data.after.data.detections,
                ["1744534706.323662-abcdefg"]
            );
            assert_eq!(new_data.before.data.objects, ["person"]);
            assert_eq!(new_data.after.data.objects, ["person"]);
            assert_eq!(new_data.before.data.zones, ["full_frame"]);
            assert_eq!(new_data.after.data.zones, ["full_frame"]);
        }

        {
            let update_sample_data = r#"{"type": "update", "before": {"id": "1745534741.333822-vsz5s4", "camera": "CameraLabel", "start_time": 1745534741.333822, "end_time": null, "severity": "alert", "thumb_path": "/media/frigate/clips/review/thumb-CameraLabel-1745534741.333822-vsz5s4.webp", "data": {"detections": ["1744534706.323662-abcdefg"], "objects": ["person"], "sub_labels": [], "zones": ["full_frame"], "audio": []}}, "after": {"id": "1745534741.333822-vsz5s4", "camera": "CameraLabel", "start_time": 1745534741.333822, "end_time": null, "severity": "alert", "thumb_path": "/media/frigate/clips/review/thumb-CameraLabel-1745534741.333822-vsz5s4.webp", "data": {"detections": ["1744534706.323662-abcdefg"], "objects": ["person"], "sub_labels": [], "zones": ["full_frame"], "audio": []}}}"#;
            let update_data = serde_json::from_str::<ReviewsPayload>(&update_sample_data).unwrap();
            assert_eq!(update_data.type_field, TypeField::Update);
            assert_eq!(update_data.before.camera, "CameraLabel");
            assert_eq!(update_data.after.camera, "CameraLabel");
            assert_eq!(update_data.before.id, "1745534741.333822-vsz5s4");
            assert_eq!(update_data.after.id, "1745534741.333822-vsz5s4");
            assert_eq!(update_data.before.end_time, None);
            assert_eq!(update_data.after.end_time, None);
            assert_eq!(update_data.before.severity, "alert");
            assert_eq!(update_data.after.severity, "alert");
            assert_eq!(
                update_data.before.thumb_path,
                "/media/frigate/clips/review/thumb-CameraLabel-1745534741.333822-vsz5s4.webp"
            );
            assert_eq!(
                update_data.after.thumb_path,
                "/media/frigate/clips/review/thumb-CameraLabel-1745534741.333822-vsz5s4.webp"
            );
            assert_eq!(
                update_data.before.data.detections,
                ["1744534706.323662-abcdefg"]
            );
            assert_eq!(
                update_data.before.data.detections,
                ["1744534706.323662-abcdefg"]
            );
            assert_eq!(
                update_data.after.data.detections,
                ["1744534706.323662-abcdefg"]
            );
            assert_eq!(update_data.before.data.objects, ["person"]);
            assert_eq!(update_data.after.data.objects, ["person"]);
            assert_eq!(update_data.before.data.zones, ["full_frame"]);
            assert_eq!(update_data.after.data.zones, ["full_frame"]);
        }

        {
            let end_sample_data = r#"{"type": "end", "before": {"id": "1745534741.333822-vsz5s4", "camera": "CameraLabel", "start_time": 1745534741.333822, "end_time": null, "severity": "alert", "thumb_path": "/media/frigate/clips/review/thumb-CameraLabel-1745534741.333822-vsz5s4.webp", "data": {"detections": ["1744534706.323662-abcdefg"], "objects": ["person"], "sub_labels": [], "zones": ["full_frame"], "audio": []}}, "after": {"id": "1745534741.333822-vsz5s4", "camera": "CameraLabel", "start_time": 1745534741.333822, "end_time": 1756534721.13457, "severity": "alert", "thumb_path": "/media/frigate/clips/review/thumb-CameraLabel-1745534741.333822-vsz5s4.webp", "data": {"detections": ["1744534706.323662-abcdefg"], "objects": ["person"], "sub_labels": [], "zones": ["full_frame"], "audio": []}}}"#;
            let end_data = serde_json::from_str::<ReviewsPayload>(&end_sample_data).unwrap();
            assert_eq!(end_data.type_field, TypeField::End);
            assert_eq!(end_data.after.camera, "CameraLabel");
            assert_eq!(end_data.before.camera, "CameraLabel");
            assert_eq!(end_data.before.id, "1745534741.333822-vsz5s4");
            assert_eq!(end_data.after.id, "1745534741.333822-vsz5s4");
            assert_eq!(end_data.before.end_time, None);
            assert_eq!(end_data.after.end_time, Some(1756534721.13457));
            assert_eq!(end_data.before.severity, "alert");
            assert_eq!(end_data.after.severity, "alert");
            assert_eq!(
                end_data.before.thumb_path,
                "/media/frigate/clips/review/thumb-CameraLabel-1745534741.333822-vsz5s4.webp"
            );
            assert_eq!(
                end_data.after.thumb_path,
                "/media/frigate/clips/review/thumb-CameraLabel-1745534741.333822-vsz5s4.webp"
            );
            assert_eq!(
                end_data.before.data.detections,
                ["1744534706.323662-abcdefg"]
            );
            assert_eq!(
                end_data.before.data.detections,
                ["1744534706.323662-abcdefg"]
            );
            assert_eq!(
                end_data.after.data.detections,
                ["1744534706.323662-abcdefg"]
            );
            assert_eq!(end_data.before.data.objects, ["person"]);
            assert_eq!(end_data.after.data.objects, ["person"]);
            assert_eq!(end_data.before.data.zones, ["full_frame"]);
            assert_eq!(end_data.after.data.zones, ["full_frame"]);
        }
    }
}
