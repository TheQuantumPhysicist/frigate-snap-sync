# Frigate Snap Sync

Frigate Snap Sync is a program that works in tandem with Frigate. It responds to Frigate when a snapshot or a review is made (and more can be added), and uploads them to a server(s) of choice.

Frigate Snap Sync uses the mqtt protocol from Frigate to respond to events from Frigate.

## Configuration

A configuration file is required to run Snap Sync. An example file can be found in this repository in [config.yaml.example](/config.yaml.example).

Copy that example config file to `config.yaml`, modify it, then use it.

## Running the program

The program is written in Rust, so it can be built easily by first [installing Rust](https://www.rust-lang.org/tools/install), then cloning the repository, then running with:

```
cargo run --release -- start
```

or you can build and use the executable:

```
cargo build --release
./target/release/snap-sync start
```

Notice that the executable after build is located in `target/release/snap-sync`.

To see the available command line arguments:

```
cargo run --release -- start --help
```

or from a build:

```
./snap-sync start --help
```

where `start` is the subcommand to start the program. Currently it is the only one. More might be added in the future.

The default configuration file is expected to be in the current directory, in the file with name `config.yaml`. You can use the command line argument `--config-file-path` or `-c`. For example:

```
cargo run --release -- start -c my-config.yaml
```

or

```
./snap-sync start -c my-config.yaml
```

## How does it look like while it is running?

You just see the logs of what is happening in the program. You can tweak the logging level using the environment variable `RUST_LOG=info` or `RUST_LOG=debug` or `RUST_LOG=trace`, etc. Usually `info` is enough, and is the default. Snap Sync uses the [tracing library](https://docs.rs/tracing/latest/tracing/) for logging.

The program is very tolerant of errors. At start, it will attempt to test the Frigate API server and will attempt to connect to the Mqtt server. If it fails, it will notify you, but it will continue and keep trying. This design is by choice to ensure that intermittent failures do not interrupt the operation of Snap Sync. It is not the responsibility of Snap Sync to ensure that Frigate is running correctly.

Meaning: Once you start Snap Sync the first time, make sure you're not seeing any errors. Then you can forget about it. It will just work.

## When does an upload occur?

- When snapshots are enabled AND a snapshot is detected, it will upload the snapshot.
- When recordings are enabled AND a recording is created, it will upload the recording.

Snap Sync automatically updates its internal state when a change in snapshots or recordings state is detected to decide to upload or ignore recordings and snapshots.

## Running in Docker

See the example `docker-compose` file in the [docker](./docker/) directory. You should ensure that Frigate, Mosquitto and Snap-Sync are all within one swarm/compose group. This is because snap-sync requires network access to Mosquitto broker to get updates, and requires access to Frigate to retrieve video clips.

### Docker image updates

With or without software updates, the container updates every day to ensure that the debian being used in the image is the latest one. The container even runs `apt upgrade` on every run to ensure that all libraries are updated.

## Scalability

This program is written to be virtually infinitely scalable, as much as you have processing power and bandwidth to upload. It is highly parallelizable (using Rust async) and can run on as many threads as needed. By default, it will use all the threads available. Obviously, it will not occupy them unless needed, as it is light-weight.

## Security

This software does not require opening or listening on any ports. No security measures are needed. Besides, this software is written with no unsafe code and has high standards for safety.

The only note to be made is that the private(s) for accessing the storage server(s). It is assumed that you're using a dedicated remote server for your data storage. It is not recommended, for example, to use the same private key/identity file that you use on your public server that contains sensitive data. You can always spin up new ssh servers for data storage. Even better, do it through a dedicated VPN layer.

The authors and contributors do not assume any responsibility for the any usage of this software, intended or not.

## Contribution

I wrote this program to solve a problem I had. You're welcome to contribute. But please make sure to maintain the same code quality you'll see in the code. I like the saying "test everything like hell", and I try to follow this mantra as much as I can.

## License

This program is licensed under the [GPLv3 license](/LICENSE).
