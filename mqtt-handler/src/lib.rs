use config::MqttHandlerConfig;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio::sync::{mpsc::UnboundedSender, oneshot};
use types::CapturedPayloads;

pub mod config;
pub mod types;

pub struct MqttHandler {
    task_handle: Option<tokio::task::JoinHandle<()>>,
    stop_sender: Option<oneshot::Sender<()>>,
}

impl MqttHandler {
    pub fn new(
        config: MqttHandlerConfig,
        data_sender: UnboundedSender<CapturedPayloads>,
    ) -> anyhow::Result<Self> {
        let mqtt_options = (&config).try_into()?;
        let (stop_sender, stop_receiver) = oneshot::channel();
        let task_handle = tokio::task::spawn(launch_eventloop(
            data_sender,
            mqtt_options,
            config,
            stop_receiver,
        ));
        Ok(Self {
            task_handle: Some(task_handle),
            stop_sender: Some(stop_sender),
        })
    }

    pub async fn wait(&mut self) {
        self.task_handle
            .take()
            .expect("Must exist")
            .await
            .expect("Awaiting mqtt failed");
    }

    pub fn stop(&mut self) {
        self.stop_sender
            .take()
            .expect("Stop called more than once")
            .send(())
            .expect("Sending stop signal failed");
    }
}

async fn launch_eventloop(
    data_sender: tokio::sync::mpsc::UnboundedSender<CapturedPayloads>,
    mqtt_options: MqttOptions,
    config: MqttHandlerConfig,
    mut stop_receiver: oneshot::Receiver<()>,
) {
    tracing::info!(
        "Connecting to mqtt server: {}:{}",
        mqtt_options.broker_address().0,
        mqtt_options.broker_address().1,
    );

    let (client, mut eventloop) = AsyncClient::new(mqtt_options, 100);

    let topic = format!("{}/#", config.mqtt_frigate_topic_prefix);

    tracing::info!("Subscribing to topic: {topic}");

    client.subscribe(topic, QoS::ExactlyOnce).await.unwrap();

    loop {
        match stop_receiver.try_recv() {
            Ok(()) => break,
            Err(e) => match e {
                oneshot::error::TryRecvError::Empty => (),
                oneshot::error::TryRecvError::Closed => break,
            },
        }

        if let Ok(notification) = eventloop.poll().await {
            if let Event::Incoming(notification) = notification {
                println!("Received = {notification:?}");
                match notification {
                    Packet::Publish(publish) => {
                        if let Some(data) = CapturedPayloads::from_publish(
                            &config,
                            &publish.topic,
                            &publish.payload,
                        ) {
                            tracing::debug!("Found relevant data from topic: {}", publish.topic);
                            data_sender.send(data).expect("Sending data message failed");
                        }
                        let payload_str = String::from_utf8(publish.payload.to_vec())
                            .unwrap_or_else(|_e| "<Payload decode failed>".to_string());
                        println!("Topic: {}", publish.topic);
                        println!("Payload: {payload_str}");
                    }
                    Packet::Connect(_)
                    | Packet::ConnAck(_)
                    | Packet::PubAck(_)
                    | Packet::PubRec(_)
                    | Packet::PubRel(_)
                    | Packet::PubComp(_)
                    | Packet::Subscribe(_)
                    | Packet::SubAck(_)
                    | Packet::Unsubscribe(_)
                    | Packet::UnsubAck(_)
                    | Packet::PingReq
                    | Packet::PingResp
                    | Packet::Disconnect => (),
                }
            }
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

fn set_credentials(
    config: &MqttHandlerConfig,
    mqtt_options: &mut MqttOptions,
) -> anyhow::Result<()> {
    match (&config.mqtt_username, &config.mqtt_password) {
        (Some(u), Some(p)) => {
            tracing::info!("Setting username and password for mqtt connection");
            mqtt_options.set_credentials(u, p);
        }
        (None, None) => {
            tracing::info!("No username and password used for mqtt connection");
        }
        (_, _) => {
            return Err(anyhow::anyhow!(
                "Username and password must be either both specified or both unspecified"
            ));
        }
    }

    Ok(())
}

impl TryFrom<&MqttHandlerConfig> for MqttOptions {
    type Error = anyhow::Error;

    fn try_from(config: &MqttHandlerConfig) -> Result<Self, Self::Error> {
        let mut mqtt_options =
            MqttOptions::new(&config.mqtt_client_id, &config.mqtt_host, config.mqtt_port);
        mqtt_options.set_max_packet_size(1 << 24, 1 << 24);
        mqtt_options.set_keep_alive(std::time::Duration::from_secs(
            config.mqtt_keep_alive_seconds,
        ));

        set_credentials(config, &mut mqtt_options)?;

        Ok(mqtt_options)
    }
}
