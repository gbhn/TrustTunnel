use std::io;
use std::io::ErrorKind;
use std::sync::Arc;
use rustls::{Certificate, PrivateKey, ServerConfig};
use tokio::net::TcpStream;
use tokio_rustls::{LazyConfigAcceptor, StartHandshake};
use tokio_rustls::server::TlsStream;
use crate::{log_utils, tls_demultiplexer};


pub(crate) struct TlsListener {}

pub(crate) struct TlsAcceptor {
    inner: StartHandshake<TcpStream>,
}

impl TlsListener {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn listen(&self, stream: TcpStream) -> io::Result<TlsAcceptor> {
        LazyConfigAcceptor::new(rustls::server::Acceptor::default(), stream)
            .await
            .map(|hs| TlsAcceptor {
                inner: hs,
            })
    }
}

impl TlsAcceptor {
    pub fn sni(&self) -> Option<String> {
        self.inner.client_hello().server_name().map(String::from)
    }

    pub fn alpn(&self) -> Vec<Vec<u8>> {
        self.inner.client_hello()
            .alpn()
            .map(|x| x.map(Vec::from).collect())
            .unwrap_or_default()
    }

    pub async fn accept(
        self,
        protocol: tls_demultiplexer::Protocol,
        cert_chain: Vec<Certificate>,
        key: PrivateKey,
        _log_id: &log_utils::IdChain<u64>,
    ) -> io::Result<TlsStream<TcpStream>> {
        let tls_config = {
            let mut cfg = ServerConfig::builder()
                .with_safe_defaults()
                .with_no_client_auth()
                .with_single_cert(cert_chain, key)
                .map_err(|e| io::Error::new(
                    ErrorKind::Other, format!("Failed to create TLS configuration: {}", e))
                )?;

            cfg.alpn_protocols = vec![protocol.as_alpn().as_bytes().to_vec()];
            Arc::new(cfg)
        };

        self.inner.into_stream(tls_config).await
    }
}
