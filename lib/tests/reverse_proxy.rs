use std::future::Future;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use bytes::Bytes;
use http::{Request, Response};
use log::info;
use tokio::net::TcpListener;
use vpn_libs_endpoint::settings::{Http1Settings, Http2Settings, ListenProtocolSettings, QuicSettings, ReverseProxySettings, Settings, TlsHostInfo, TlsHostsSettings};

mod common;

macro_rules! reverse_proxy_tests {
    ($($name:ident: $client_fn:expr,)*) => {
    $(
        #[tokio::test]
        async fn $name() {
            common::set_up_logger();
            let endpoint_address = common::make_endpoint_address();
            let (proxy_address, proxy_task) = run_proxy();

            let client_task = async {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let (response, body) = $client_fn(&endpoint_address).await;
                assert_eq!(response.status, http::StatusCode::OK);
                assert_eq!(body.as_ref(), b"how much watch?");
            };

            tokio::select! {
                _ = run_endpoint(&endpoint_address, &proxy_address) => unreachable!(),
                _ = proxy_task => unreachable!(),
                _ = client_task => (),
                _ = tokio::time::sleep(Duration::from_secs(10)) => panic!("Timed out"),
            }
        }
    )*
    }
}

reverse_proxy_tests! {
    sni_h1: sni_h1_client,
    sni_h3: sni_h3_client,
    path_h1: path_h1_client,
    path_h3: path_h3_client,
}

async fn sni_h1_client(endpoint_address: &SocketAddr) -> (http::response::Parts, Bytes) {
    let stream = common::establish_tls_connection(
        &format!("hello.{}", common::MAIN_DOMAIN_NAME),
        endpoint_address,
        None,
    ).await;

    common::do_get_request(
        stream,
        http::Version::HTTP_11,
        &format!("https://hello.{}:{}", common::MAIN_DOMAIN_NAME, endpoint_address.port()),
        &[],
    ).await
}

async fn sni_h3_client(endpoint_address: &SocketAddr) -> (http::response::Parts, Bytes) {
    let mut conn = common::Http3Session::connect(
        endpoint_address,
        &format!("hello.{}", common::MAIN_DOMAIN_NAME),
        None,
    ).await;

    conn.exchange(
        Request::get(
            format!("https://hello.{}:{}", common::MAIN_DOMAIN_NAME, endpoint_address.port())
        ).body(hyper::Body::empty()).unwrap()
    ).await
}

async fn path_h1_client(endpoint_address: &SocketAddr) -> (http::response::Parts, Bytes) {
    let stream = common::establish_tls_connection(
        common::MAIN_DOMAIN_NAME,
        endpoint_address,
        None,
    ).await;

    common::do_get_request(
        stream,
        http::Version::HTTP_11,
        &format!("https://{}:{}/hello/haha", common::MAIN_DOMAIN_NAME, endpoint_address.port()),
        &[(http::header::UPGRADE.as_str(), "1")],
    ).await
}

async fn path_h3_client(endpoint_address: &SocketAddr) -> (http::response::Parts, Bytes) {
    let mut conn = common::Http3Session::connect(
        endpoint_address,
        common::MAIN_DOMAIN_NAME,
        None,
    ).await;

    conn.exchange(
        Request::get(
            format!("https://{}:{}/hello/haha", common::MAIN_DOMAIN_NAME, endpoint_address.port()),
        ).body(hyper::Body::empty()).unwrap()
    ).await
}

async fn run_endpoint(endpoint_address: &SocketAddr, proxy_address: &SocketAddr) {
    let settings = Settings::builder()
        .listen_address(endpoint_address).unwrap()
        .listen_protocols(ListenProtocolSettings {
            http1: Some(Http1Settings::builder().build()),
            http2: Some(Http2Settings::builder().build()),
            quic: Some(QuicSettings::builder().build()),
        })
        .reverse_proxy(
            ReverseProxySettings::builder()
                .server_address(proxy_address).unwrap()
                .path_mask("/hello".to_string())
                .build().unwrap()
        )
        .allow_private_network_connections(true)
        .build().unwrap();

    let cert_key_file = common::make_cert_key_file();
    let cert_key_path = cert_key_file.path.to_str().unwrap();
    let hosts_settings = TlsHostsSettings::builder()
        .main_hosts(vec![TlsHostInfo {
            hostname: common::MAIN_DOMAIN_NAME.to_string(),
            cert_chain_path: cert_key_path.to_string(),
            private_key_path: cert_key_path.to_string(),
        }])
        .reverse_proxy_hosts(vec![TlsHostInfo {
            hostname: format!("hello.{}", common::MAIN_DOMAIN_NAME),
            cert_chain_path: cert_key_path.to_string(),
            private_key_path: cert_key_path.to_string(),
        }])
        .build().unwrap();

    common::run_endpoint_with_settings(settings, hosts_settings).await;
}

fn run_proxy() -> (SocketAddr, impl Future<Output=()>) {
    let server = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let server_addr = server.local_addr().unwrap();
    (
        server_addr,
        async move {
            let (socket, peer) = TcpListener::from_std(server).unwrap().accept().await.unwrap();
            info!("New connection from {}", peer);
            hyper::server::conn::Http::new()
                .http1_only(true)
                .serve_connection(socket, hyper::service::service_fn(request_handler))
                .await.unwrap();
        },
    )
}

async fn request_handler(request: Request<hyper::Body>) -> Result<Response<hyper::Body>, hyper::Error> {
    info!("Received request: {:?}", request);
    Ok(Response::builder().body(hyper::Body::from("how much watch?")).unwrap())
}
