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

    /// returns a future that awaits exiting the inner task of mqtt
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

// async fn on_connect(client: rumqttc::AsyncClient) {}

async fn launch_eventloop(
    data_sender: tokio::sync::mpsc::UnboundedSender<CapturedPayloads>,
    mqtt_options: MqttOptions,
    config: MqttHandlerConfig,
    mut stop_receiver: oneshot::Receiver<()>,
) {
    const MQTT_ASYNC_CAP: usize = 1000;

    tracing::info!(
        "Mqtt client targeting server: {}:{}",
        mqtt_options.broker_address().0,
        mqtt_options.broker_address().1,
    );

    let (client, mut eventloop) = AsyncClient::new(mqtt_options, MQTT_ASYNC_CAP);

    let topic = format!("{}/#", config.mqtt_frigate_topic_prefix);

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
                match notification {
                    Packet::Publish(publish) => {
                        if let Some(data) = CapturedPayloads::from_publish(
                            &config,
                            &publish.topic,
                            &publish.payload,
                        ) {
                            tracing::debug!("Found relevant data from topic: `{}`", publish.topic);
                            data_sender.send(data).expect("Sending data message failed");
                        } else {
                            tracing::debug!("Ignoring data with topic: `{}`", publish.topic);
                        }
                    }

                    // On connection, an acknowledgement is sent to the client (us here).
                    Packet::ConnAck(conn_ack) => {
                        // When conn_ack.session_present is `false`, we need to resubscribe.
                        //
                        // From the docs:
                        //
                        // > Session Present (Bit 0): Used to indicate whether the server is using an existing session to resume
                        // > communication with the client.
                        // > Session Present may be 1 only when the client has set Clean Start to 0 in the CONNECT connection.
                        //
                        // Source: https://emqx.medium.com/mqtt-5-0-packet-explained-01-connect-connack-f941e5c0c61b
                        if !conn_ack.session_present {
                            tracing::info!(
                                "On connection acknowledgement, session is not present in mqtt. (Re)subscribing to topic: `{topic}`"
                            );

                            match client.subscribe(topic.clone(), QoS::ExactlyOnce).await {
                                Ok(()) => {
                                    tracing::info!("Subscription request of `{topic}` is sent");
                                }
                                Err(e) => {
                                    tracing::info!(
                                        "Subscription to topic `{topic}` sending failed: `{e}`"
                                    );
                                }
                            }
                        }
                    }

                    Packet::SubAck(_sub_ack) => {
                        tracing::info!("Mqtt server acknowledged: Topic subscription successful!");
                    }

                    Packet::Connect(_)
                    | Packet::Disconnect
                    | Packet::PubAck(_)
                    | Packet::PubRec(_)
                    | Packet::PubRel(_)
                    | Packet::PubComp(_)
                    | Packet::Subscribe(_)
                    | Packet::Unsubscribe(_)
                    | Packet::UnsubAck(_)
                    | Packet::PingReq
                    | Packet::PingResp => (),
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
