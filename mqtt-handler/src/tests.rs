use super::*;
use bytes::Bytes;

// Builds a real payload the same way the event loop does, so the forwarding
// helper is exercised against a genuine `CapturedPayloads` value.
fn sample_payload() -> CapturedPayloads {
    let config = MqttHandlerConfig {
        mqtt_frigate_topic_prefix: "frigate".to_string(),
        ..Default::default()
    };

    CapturedPayloads::from_publish(
        &config,
        "frigate/front_door/recordings/state",
        &Bytes::from_static(b"ON"),
    )
    .expect("a recordings-state topic with an ON payload must parse")
}

#[test]
fn forward_to_live_receiver_continues_and_delivers() {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();

    let flow = forward_captured_payload(&sender, sample_payload());

    assert_eq!(flow, EventLoopFlow::Continue);
    assert!(
        receiver.try_recv().is_ok(),
        "the payload must reach the receiver"
    );
}

#[test]
fn forward_to_dropped_receiver_signals_stop_without_panicking() {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    // The consuming system shut down and dropped its end of the channel.
    drop(receiver);

    // Before the fix this path called `send(...).expect(...)`, which panicked
    // and, under `panic = "abort"`, took the whole process down during a normal
    // Ctrl+C shutdown. It must now report a stop instead.
    let flow = forward_captured_payload(&sender, sample_payload());

    assert_eq!(flow, EventLoopFlow::Stop);
}
