FROM rust:1.96-bookworm AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
RUN cargo build --locked --release

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /nonexistent --shell /usr/sbin/nologin cnshell \
    && install -d -o cnshell -g cnshell /data
COPY --from=build /build/target/release/cnshell-team-relay /usr/local/bin/cnshell-team-relay
USER 10001:10001
EXPOSE 8787
ENTRYPOINT ["/usr/local/bin/cnshell-team-relay"]
