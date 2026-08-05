# We use a separate build container so we don't install all build dependencies
# in the final image
FROM rust:slim-bookworm AS builder
ARG VERSION

RUN DEBIAN_FRONTEND=noninteractive apt-get update && \
    apt-get full-upgrade -y && \
    apt-get install -y git build-essential cmake pkg-config libssl-dev && \
    apt-get clean && \
    rm -fR /var/lib/apt/lists

RUN git clone https://github.com/TheQuantumPhysicist/frigate-snap-sync /tmp/build

WORKDIR /tmp/build

# If a version was specified we checkout the matching tag
RUN if [[ -n "${VERSION}" ]] ; then git checkout "v${VERSION}" ; fi

RUN cargo build --release --all --locked

# This will be the final image that will be published
FROM debian:bookworm-slim

ENV LANG=en_US.UTF-8
ENV LC_ALL=en_US.UTF-8
ENV LC_CTYPE=en_US.UTF-8
ENV LC_ALL=en_US.UTF-8
ENV LANGUAGE=en_US.UTF-8

RUN echo "LC_ALL=en_US.UTF-8" >> /etc/environment

# Only install runtime dependencies here
RUN DEBIAN_FRONTEND=noninteractive apt-get update && \
    apt-get full-upgrade -y && \
    apt-get install -y libssl3 && \
    apt-get clean && \
    rm -fR /var/lib/apt/lists

# The matching group will be automatically created
RUN useradd --uid 10246 -m runner

USER runner

RUN mkdir /home/runner/app/
COPY --from=builder /tmp/build/target/release/snap-sync /home/runner/app/snap-sync

WORKDIR /home/runner/app/

ENTRYPOINT ["/home/runner/app/snap-sync"]
CMD ["start",  "-c",  "/config.yaml" ]
