use log::{debug, error, info, warn, LevelFilter};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::signal;
use trusttunnel::authentication::registry_based::RegistryBasedAuthenticator;
use trusttunnel::authentication::Authenticator;
use trusttunnel::client_config;
use trusttunnel::core::Core;
use trusttunnel::settings::Settings;
use trusttunnel::shutdown::Shutdown;
use trusttunnel::{log_utils, settings};

const VERSION_STRING: &str = env!("CARGO_PKG_VERSION");
const VERSION_PARAM_NAME: &str = "v_e_r_s_i_o_n_do_not_change_this_name_it_will_break";
const LOG_LEVEL_PARAM_NAME: &str = "log_level";
const LOG_FILE_PARAM_NAME: &str = "log_file";
const SETTINGS_PARAM_NAME: &str = "settings";
const TLS_HOSTS_SETTINGS_PARAM_NAME: &str = "tls_hosts_settings";
const CLIENT_CONFIG_PARAM_NAME: &str = "client_config";
const ADDRESS_PARAM_NAME: &str = "address";
const CUSTOM_SNI_PARAM_NAME: &str = "custom_sni";
const SENTRY_DSN_PARAM_NAME: &str = "sentry_dsn";
const THREADS_NUM_PARAM_NAME: &str = "threads_num";

/// Load credentials from a TOML file and return an authenticator if clients are found.
fn load_authenticator_from_file(path: &str) -> Result<Option<Arc<dyn Authenticator>>, String> {
    use trusttunnel::authentication::registry_based::Client;

    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Couldn't read credentials file '{}': {}", path, e))?;
    let doc: toml::Table = content
        .parse()
        .map_err(|e| format!("Couldn't parse credentials file '{}': {}", path, e))?;

    let clients: Vec<Client> = match doc.get("client") {
        Some(toml::Value::Array(arr)) => arr
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let table = entry
                    .as_table()
                    .ok_or_else(|| format!("Client #{}: expected a table", i + 1))?;
                let username = table
                    .get("username")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| format!("Client #{}: missing username", i + 1))?
                    .to_string();
                let password = table
                    .get("password")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| format!("Client #{}: missing password", i + 1))?
                    .to_string();
                Ok(Client { username, password })
            })
            .collect::<Result<Vec<_>, String>>()?,
        _ => return Ok(None),
    };

    if clients.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Arc::new(RegistryBasedAuthenticator::new(&clients))))
    }
}

#[cfg(unix)]
fn increase_fd_limit() {
    use nix::sys::resource::{getrlimit, setrlimit, Resource};
    let max_rlim = 65536;

    let (soft, hard) = match getrlimit(Resource::RLIMIT_NOFILE) {
        Ok(limits) => limits,
        Err(err) => {
            warn!("Failed to get file descriptor limit: {}", err);
            return;
        }
    };

    let target_limit = std::cmp::min(hard, max_rlim);
    if soft >= target_limit {
        debug!(
            "File descriptor limit is already {} (target: {})",
            soft, target_limit
        );
        return;
    }

    if let Err(err) = setrlimit(Resource::RLIMIT_NOFILE, target_limit, hard) {
        warn!(
            "Failed to increase file descriptor limit from {} to {}: {}",
            soft, target_limit, err
        );
        return;
    }

    debug!(
        "Successfully increased file descriptor limit to {}",
        target_limit
    );
}

#[cfg(not(unix))]
fn increase_fd_limit() {}

fn main() {
    let args = clap::Command::new("VPN endpoint")
        .args(&[
            // Built-in version parameter handling is deficient in that it
            // outputs `<program name> <version>` instead of just `<version>`
            // and also uses `-V` instead of `-v` as the shorthand.
            clap::Arg::new(VERSION_PARAM_NAME)
                .short('v')
                .long("version")
                .action(clap::ArgAction::SetTrue)
                .help("Print the version of this software and exit"),
            clap::Arg::new(LOG_LEVEL_PARAM_NAME)
                .short('l')
                .long("loglvl")
                .action(clap::ArgAction::Set)
                .value_parser(["info", "debug", "trace"])
                .default_value("info")
                .help("Logging level"),
            clap::Arg::new(LOG_FILE_PARAM_NAME)
                .long("logfile")
                .action(clap::ArgAction::Set)
                .help("File path for storing logs. If not specified, the logs are printed to stdout"),
            clap::Arg::new(SENTRY_DSN_PARAM_NAME)
                .long(SENTRY_DSN_PARAM_NAME)
                .action(clap::ArgAction::Set)
                .help("Sentry DSN (see https://docs.sentry.io/product/sentry-basics/dsn-explainer/ for details)"),
            clap::Arg::new(THREADS_NUM_PARAM_NAME)
                .long("jobs")
                .action(clap::ArgAction::Set)
                .value_parser(clap::value_parser!(usize))
                .help("The number of worker threads. If not specified, set to the number of CPUs on the machine."),
            clap::Arg::new(SETTINGS_PARAM_NAME)
                .action(clap::ArgAction::Set)
                .required_unless_present(VERSION_PARAM_NAME)
                .help("Path to a settings file"),
            clap::Arg::new(TLS_HOSTS_SETTINGS_PARAM_NAME)
                .action(clap::ArgAction::Set)
                .required_unless_present(VERSION_PARAM_NAME)
                .help("Path to a file containing TLS hosts settings. Sending SIGHUP to the process causes reloading the settings."),
            clap::Arg::new(CLIENT_CONFIG_PARAM_NAME)
                .action(clap::ArgAction::Set)
                .requires(ADDRESS_PARAM_NAME)
                .short('c')
                .long("client_config")
                .value_names(["client_name"])
                .help("Print the endpoint config for specified client and exit."),
            clap::Arg::new(ADDRESS_PARAM_NAME)
                .action(clap::ArgAction::Append)
                .requires(CLIENT_CONFIG_PARAM_NAME)
                .short('a')
                .long("address")
                .help("Endpoint address to be added to client's config."),
            clap::Arg::new(CUSTOM_SNI_PARAM_NAME)
                .action(clap::ArgAction::Set)
                .requires(CLIENT_CONFIG_PARAM_NAME)
                .short('s')
                .long("custom-sni")
                .help("Custom SNI override for client connection. Must match an allowed_sni in hosts.toml.")
        ])
        .disable_version_flag(true)
        .get_matches();

    if args.contains_id(VERSION_PARAM_NAME)
        && Some(true) == args.get_one::<bool>(VERSION_PARAM_NAME).copied()
    {
        println!("{}", VERSION_STRING);
        return;
    }

    #[cfg(feature = "tracing")]
    console_subscriber::init();

    let _guard = args.get_one::<String>(SENTRY_DSN_PARAM_NAME).map(|x| {
        sentry::init((
            x.clone(),
            sentry::ClientOptions {
                release: sentry::release_name!(),
                ..Default::default()
            },
        ))
    });

    let _guard = log_utils::LogFlushGuard;
    log::set_logger(match args.get_one::<String>(LOG_FILE_PARAM_NAME) {
        None => log_utils::make_stdout_logger(),
        Some(file) => log_utils::make_file_logger(file).expect("Couldn't open the logging file"),
    })
    .expect("Couldn't set logger");

    log::set_max_level(
        match args
            .get_one::<String>(LOG_LEVEL_PARAM_NAME)
            .map(String::as_str)
        {
            None => LevelFilter::Info,
            Some("info") => LevelFilter::Info,
            Some("debug") => LevelFilter::Debug,
            Some("trace") => LevelFilter::Trace,
            Some(x) => panic!("Unexpected log level: {}", x),
        },
    );

    increase_fd_limit();

    let settings_path = args.get_one::<String>(SETTINGS_PARAM_NAME).unwrap();
    let settings_content =
        std::fs::read_to_string(settings_path).expect("Couldn't read the settings file");

    // Extract the credentials_file path from raw TOML before Settings consumes it,
    // so we can re-read it on SIGHUP for credential reloading.
    let credentials_file_path: Option<String> = settings_content
        .parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("credentials_file")?.as_str().map(String::from));

    let settings: Settings =
        toml::from_str(&settings_content).expect("Couldn't parse the settings file");

    if settings.get_clients().is_empty() && settings.get_listen_address().ip().is_loopback() {
        warn!(
            "No credentials configured (credentials_file is missing). \
            Anyone can connect to this endpoint. This is acceptable for local development \
            but should not be used in production."
        );
    }

    let tls_hosts_settings_path = args
        .get_one::<String>(TLS_HOSTS_SETTINGS_PARAM_NAME)
        .unwrap();
    let tls_hosts_settings: settings::TlsHostsSettings = toml::from_str(
        &std::fs::read_to_string(tls_hosts_settings_path)
            .expect("Couldn't read the TLS hosts settings file"),
    )
    .expect("Couldn't parse the TLS hosts settings file");

    if args.contains_id(CLIENT_CONFIG_PARAM_NAME) {
        let username = args.get_one::<String>(CLIENT_CONFIG_PARAM_NAME).unwrap();
        let addresses: Vec<SocketAddr> = args
            .get_many::<String>(ADDRESS_PARAM_NAME)
            .expect("At least one address should be specified")
            .map(|x| {
                SocketAddr::from_str(x)
                    .or_else(|_| {
                        SocketAddr::from_str(&format!("{}:{}", x, settings.get_listen_address().port()))
                    })
                    .unwrap_or_else(|_| {
                        panic!("Failed to parse address. Expected `ip` or `ip:port` format, found: `{}`", x);
                    })
            })
            .collect();

        let custom_sni = args.get_one::<String>(CUSTOM_SNI_PARAM_NAME).cloned();
        if let Some(ref sni) = custom_sni {
            let is_valid = tls_hosts_settings
                .get_main_hosts()
                .iter()
                .any(|host| host.hostname == *sni || host.allowed_sni.contains(sni));
            if !is_valid {
                eprintln!(
                    "Error: custom SNI '{}' does not match any hostname or allowed_sni in hosts.toml",
                    sni
                );
                std::process::exit(1);
            }
        }

        let client_config = client_config::build(
            username,
            addresses,
            settings.get_clients(),
            &tls_hosts_settings,
            custom_sni,
        );
        println!("{}", client_config.compose_toml());
        return;
    }

    let rt = {
        let mut builder = tokio::runtime::Builder::new_multi_thread();
        builder.enable_io();
        builder.enable_time();

        if let Some(n) = args.get_one::<usize>(THREADS_NUM_PARAM_NAME) {
            builder.worker_threads(*n);
        }

        builder.build().expect("Failed to set up runtime")
    };

    let shutdown = Shutdown::new();
    let authenticator: Option<Arc<dyn Authenticator>> = if !settings.get_clients().is_empty() {
        Some(Arc::new(RegistryBasedAuthenticator::new(
            settings.get_clients(),
        )))
    } else {
        None
    };
    let core = Arc::new(
        Core::new(
            settings,
            authenticator,
            tls_hosts_settings,
            shutdown.clone(),
        )
        .expect("Couldn't create core instance"),
    );

    let listen_task = {
        let core = core.clone();
        async move { core.listen().await }
    };

    let reload_tls_hosts_task = {
        let tls_hosts_settings_path = tls_hosts_settings_path.clone();
        let credentials_file_path = credentials_file_path.clone();
        async move {
            let mut sighup_listener = signal::unix::signal(signal::unix::SignalKind::hangup())
                .expect("Couldn't start SIGHUP listener");

            loop {
                sighup_listener.recv().await;
                info!("Reloading TLS hosts settings");

                let tls_hosts_settings: settings::TlsHostsSettings = toml::from_str(
                    &std::fs::read_to_string(&tls_hosts_settings_path)
                        .expect("Couldn't read the TLS hosts settings file"),
                )
                .expect("Couldn't parse the TLS hosts settings file");

                core.reload_tls_hosts_settings(tls_hosts_settings)
                    .expect("Couldn't apply new settings");
                info!("TLS hosts settings are successfully reloaded");

                if let Some(ref creds_path) = credentials_file_path {
                    info!("Reloading credentials");
                    match load_authenticator_from_file(creds_path) {
                        Ok(authenticator) => {
                            core.reload_authenticator(authenticator);
                            info!("Credentials are successfully reloaded");
                        }
                        Err(e) => {
                            error!("Failed to reload credentials: {}", e);
                        }
                    }
                }
            }
        }
    };

    #[allow(clippy::await_holding_lock)]
    let interrupt_task = async move {
        tokio::signal::ctrl_c().await.unwrap();
        shutdown.lock().unwrap().submit();
        shutdown.lock().unwrap().completion().await
    };

    let exit_code = rt.block_on(async move {
        tokio::select! {
            listen_result = listen_task => match listen_result {
                Ok(()) => 0,
                Err(e) => {
                    error!("Error while listening IO events: {}", e);
                    1
                }
            },
            _ = reload_tls_hosts_task => {
                error!("Error while reloading TLS hosts");
                1
            },
            _ = interrupt_task => {
                info!("Interrupted by user");
                0
            },
        }
    });

    std::process::exit(exit_code);
}
