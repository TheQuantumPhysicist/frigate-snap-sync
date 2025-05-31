FROM debian:latest
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update
RUN apt-get install -y locales locales-all
ENV LANG=en_US.UTF-8
ENV LC_ALL=en_US.UTF-8
ENV LC_CTYPE=en_US.UTF-8
ENV LC_ALL=en_US.UTF-8
ENV LANGUAGE=en_US.UTF-8

RUN echo "LC_ALL=en_US.UTF-8" >> /etc/environment
RUN apt-get update
RUN apt-get upgrade -y
RUN apt-get install -y curl tmux nano net-tools procps htop git build-essential cmake pkg-config libssl-dev sudo

RUN groupadd --gid 10246                runner
RUN useradd  --gid 10246 --uid 10246 -m runner
RUN passwd -l runner

USER runner

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
RUN cd /home/runner/
WORKDIR /home/runner/

ENV PATH="/home/runner/.cargo/bin:$PATH"
RUN echo "export /home/runner/.cargo/bin:$PATH" > /home/runner/.bashrc

RUN git clone https://github.com/TheQuantumPhysicist/frigate-snap-sync
RUN cd /home/runner/frigate-snap-sync/
WORKDIR /home/runner/frigate-snap-sync/
RUN cargo update
RUN (cargo update && cargo build --release --all) || (git checkout Cargo.lock && cargo build --release --all --locked)

USER root

RUN echo "" >> /etc/sudoers
RUN echo "runner ALL=(ALL) NOPASSWD: /usr/bin/apt-get update" >> /etc/sudoers
RUN echo "runner ALL=(ALL) NOPASSWD: /usr/bin/apt-get clean" >> /etc/sudoers
RUN echo "runner ALL=(ALL) NOPASSWD: /usr/bin/apt-get upgrade -y" >> /etc/sudoers
RUN echo "runner ALL=(ALL) NOPASSWD: /usr/bin/apt-file update" >> /etc/sudoers

USER runner

RUN mkdir /home/runner/app/
RUN cp /home/runner/frigate-snap-sync/target/release/snap-sync /home/runner/app/
RUN cd /home/runner/app/
WORKDIR /home/runner/app/

# Clean up the source and build dirs
RUN rm -rf /home/runner/frigate-snap-sync/

CMD sudo apt-get update && sudo apt-get upgrade -y && /home/runner/app/snap-sync start -c /config.yaml
