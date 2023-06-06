# syntax=docker/dockerfile:1
FROM bench-common

ARG ENDPOINT_DIR="vpn-libs-endpoint"
ARG ENDPOINT_HOSTNAME="endpoint.bench"
ARG CONFIG_FILE="vpn.conf"
ARG TLS_HOSTS_SETTINGS_FILE="tls_hosts.conf"
ARG LOG_LEVEL="info"

RUN apt install -y openssl

COPY $ENDPOINT_DIR/lib/Cargo.toml /tmp/Cargo.toml
RUN cargo fetch --manifest-path /tmp/Cargo.toml && rm /tmp/Cargo.toml

COPY $ENDPOINT_DIR /bench/$ENDPOINT_DIR

WORKDIR /bench/
RUN cd "$ENDPOINT_DIR" && \
    cargo build --release --bin vpn_endpoint && \
    mv ./target/release/vpn_endpoint /bench/ && \
    cd examples/my_vpn && \
    openssl req -config openssl.conf -new -x509 -sha256 -newkey rsa:2048 -nodes -days 1000 \
        -keyout /bench/key.pem -out /bench/cert.pem \
        -subj "/C=de/CN=$ENDPOINT_HOSTNAME" \
        -addext "subjectAltName = DNS:*.$ENDPOINT_HOSTNAME"

RUN echo "\
listen_address = \"[::]:4433\"\n\
allow_private_network_connections = true\n\
[listen_protocols.http1]\n\
[listen_protocols.http2]\n\
[listen_protocols.quic]\n\
" >>$CONFIG_FILE

RUN echo "\
[[main_hosts]]\n\
hostname = \"$ENDPOINT_HOSTNAME\"\n\
cert_chain_path = \"cert.pem\"\n\
private_key_path = \"key.pem\"\n\
" >>$TLS_HOSTS_SETTINGS_FILE

ENV LOG_LEVEL=$LOG_LEVEL \
    CONFIG_FILE=$CONFIG_FILE \
    TLS_HOSTS_SETTINGS_FILE=$TLS_HOSTS_SETTINGS_FILE \
    RUST_BACKTRACE=1
ENTRYPOINT ./vpn_endpoint -l "$LOG_LEVEL" "$CONFIG_FILE" "$TLS_HOSTS_SETTINGS_FILE" >>/tmp/vpn.log 2>&1
