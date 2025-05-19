use std::path::PathBuf;

use randomness::{Rng, make_true_rng};

fn random_string(length: usize) -> String {
    make_true_rng()
        .sample_iter(&randomness::distributions::Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

fn current_datetime_as_string() -> String {
    chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string()
}

pub struct Podman {
    name: String,
    env: Vec<(String, String)>,
    port_mappings: Vec<(Option<u16>, u16)>,
    volumes: Vec<(PathBuf, PathBuf)>,
    container: String,
    pos_args: Vec<String>,
    stopped: bool,
}

impl Podman {
    #[must_use]
    pub fn new(name_prefix: &str, container: impl Into<String>) -> Self {
        let name = [
            name_prefix.to_string(),
            current_datetime_as_string(),
            random_string(8),
        ]
        .join("-");

        Self {
            name,
            env: Vec::new(),
            port_mappings: Vec::new(),
            volumes: Vec::new(),
            pos_args: Vec::new(),
            container: container.into(),
            stopped: false,
        }
    }

    #[must_use]
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    /// If `host_port` is `None`, the container port will be exposed on a random available host port.
    #[must_use]
    pub fn with_port_mapping(mut self, host_port: Option<u16>, container_port: u16) -> Self {
        self.port_mappings.push((host_port, container_port));
        self
    }

    #[must_use]
    pub fn with_volume_mapping(
        mut self,
        host_path: impl Into<PathBuf>,
        guest_path: impl Into<PathBuf>,
    ) -> Self {
        self.volumes.push((host_path.into(), guest_path.into()));
        self
    }

    #[must_use]
    pub fn with_positional_arg(mut self, arg: impl Into<String>) -> Self {
        self.pos_args.push(arg.into());
        self
    }

    pub fn run(&mut self) {
        let mut command = std::process::Command::new("podman");
        command.arg("run");
        command.arg("--detach");
        command.arg("--rm");
        command.arg("--name");
        command.arg(&self.name);
        for (key, value) in &self.env {
            command.arg("-e");
            command.arg(format!("{key}={value}"));
        }
        for (host_port, container_port) in &self.port_mappings {
            command.arg("-p");
            match host_port {
                Some(host_port) => command.arg(format!("{host_port}:{container_port}")),
                None => command.arg(format!("{container_port}")),
            };
        }
        for (host_path, guest_path) in &self.volumes {
            command.arg("-v");
            command.arg(format!("{}:{}", host_path.display(), guest_path.display()));
        }

        command.arg(&self.container);
        for arg in &self.pos_args {
            command.arg(arg);
        }

        tracing::info!(
            "Podman run command args: {:?}",
            command
                .get_args()
                .map(|s| s.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        );
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "Failed to run podman command: {}\n{}",
            command
                .get_args()
                .map(|s| s.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        self.stopped = false;
    }

    pub fn get_port_mapping(&self, container_port: u16) -> Option<u16> {
        let mut command = std::process::Command::new("podman");
        command.arg("port");
        command.arg(&self.name);
        command.arg(format!("{container_port}"));

        let output = command.output().unwrap();
        tracing::info!(
            "Podman ports command args: {:?}",
            command
                .get_args()
                .map(|s| s.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        );
        assert!(
            output.status.success(),
            "Failed to run podman command: {:?}\n{}",
            command,
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let line = stdout.lines().next()?;
        let parts = line.split(':').collect::<Vec<&str>>();
        let port = parts
            .get(1)
            .expect("Failed to find host port with the format 0.0.0.0:2345")
            .parse::<u16>()
            .unwrap_or_else(|e| panic!("Failed to parse host port with the format to u16: {e}"));

        Some(port)
    }

    pub fn stop(&mut self) {
        let mut command = std::process::Command::new("podman");
        command.arg("stop");
        command.arg(&self.name);
        let output = command.output().unwrap();
        tracing::info!(
            "Podman stop command args: {:?}",
            command
                .get_args()
                .map(|s| s.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        );
        assert!(
            output.status.success(),
            "Failed to run podman command: {:?}\n{}",
            command,
            String::from_utf8_lossy(&output.stderr)
        );
        self.stopped = true;
    }

    /// Uses the command `podman logs` to print the logs of the container.
    pub fn print_logs(&mut self) {
        let mut command = std::process::Command::new("podman");
        command.arg("logs");
        command.arg(&self.name);
        let output = command.output().unwrap();
        tracing::info!(
            "Podman logs command args: {:?}",
            command
                .get_args()
                .map(|s| s.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        );
        assert!(
            output.status.success(),
            "Failed to run podman command: {:?}\n{}",
            command,
            String::from_utf8_lossy(&output.stderr)
        );

        {
            let mut logs = String::new();
            logs.push_str("==================================================================\n");
            logs.push_str("==================================================================\n");
            logs.push_str("==================================================================\n");
            logs.push_str(&format!("Logs for container '{}' (stdout):\n", self.name));
            logs.push_str("==================================================================\n");
            logs.push_str(&String::from_utf8_lossy(&output.stdout));
            logs.push_str("==================================================================\n");
            logs.push_str("==================================================================\n");
            logs.push_str("==================================================================\n");
            logs.push_str(&format!("Logs for container '{}' (stderr):\n", self.name));
            logs.push_str("==================================================================\n");
            logs.push_str(&String::from_utf8_lossy(&output.stderr));
            logs.push_str("\n\n");
            logs.push_str("==================================================================\n");
            logs.push_str("==================================================================\n");
            logs.push_str("==================================================================\n");

            println!("{logs}");
        }
    }

    fn destructor(&mut self) {
        self.print_logs();
        if !self.stopped {
            self.stop();
        }
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Drop for Podman {
    fn drop(&mut self) {
        self.destructor();
    }
}
