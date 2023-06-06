# AdGuard VPN endpoint

## Building

Execute the following commands in the Terminal:
```shell
cargo build
```
to build the debug version, or
```shell
cargo build --release
```
to build the release version.

## Issuing self-signed cert and keys (RSA)

Execute the following commands in the Terminal:
```shell
openssl req -config <openssl.conf> -new -x509 -sha256 -newkey rsa:2048 -nodes -days 1000 -keyout key.pem -out cert.pem
```
where
* `<openssl.conf>` is an optional OpenSSL request template file

## Endpoint configuration

### Library configuration

An endpoint can be configured using a couple of TOML files.

#### Settings

The example file with the full set of available options and their descriptions
can be found [here](./examples/my_vpn/vpn.toml).
The file structure reflects the library settings (`struct Settings` in [settings.rs](./lib/src/settings.rs)).

#### TlsHostsSettings

The example file with the full set of available options and their descriptions
can be found [here](./examples/my_vpn/tls_hosts.toml).
The file structure reflects the TLS hosts library settings
(`struct TlsHostsSettings` in [settings.rs](./lib/src/settings.rs)).
These settings may be reloaded dynamically (see [here](#dynamic-reloading-of-tls-hosts-settings) for details).

### Executable features

#### Configuration

Some options reside on the application level. Such options can be configured via command
line arguments. For example:

* [Sentry DSN](https://docs.sentry.io/product/sentry-basics/dsn-explainer/) is configured
  by specifying `--sentry_dsn <url>`
* Logging level is configured by `[--log_level|-l] [info|debug|trace]` (`info` - default)
* Logging file is configured by `--log_file <path>`. If not specified, the instance logs
  to `stdout`.

To see the full set of available options, execute the following commands in the Terminal:
```shell
<path/to/target>/vpn_endpoint -h
```

#### Dynamic reloading of TLS hosts settings

The executable is able to reload TLS hosts settings dynamically. To trigger this, send the SIGHUP signal
to the process. After receiving, it reparses the TLS hosts settings file that was passed in arguments and
applies the new settings.

**IMPORTANT:** the file paths passed through the settings must remain valid until the process exit or until
the next reloading.

## Running

To run the binary through `cargo`, execute the following commands in the Terminal:
```shell
cargo run --bin vpn_endpoint -- <path/to/vpn.config> <path/to/tls_hosts.config>
```

To run the binary directly, execute the following commands in the Terminal:
```shell
<path/to/target>/vpn_endpoint <path/to/vpn.config> <path/to/tls_hosts.config>
```
where `<path/to/target>` is determined by the build command (by default it is `./target/debug` or
`./target/release` depending on the build type).

## Example endpoint

For a quic setup, you can run the example endpoint (see [here](./examples/my_vpn)).
It shows the essential things needed to run an instance.
To start one, run the following commands in the Terminal:
```shell
cd ./examples/my_vpn && ./run.sh
```
It may ask you to enter some information for generating your certificate.
Skip it clicking `enter` if it does not matter.

## Testing with Google Chrome

1) 2 options:
   * Add the generated certificate to the trusted store and run the Google Chrome
   * Run the Google Chrome from Terminal like this:
    ```shell
    google-chrome --ignore-certificate-errors
    ```
   **IMPORTANT:** the second option should be used just for testing, it removes the first line
                  of defence against malicious resources
2) Set up the endpoint as an HTTPS proxy server in the browser (either via browser settings or 
using an extension like `Proxy SwitchyOmega`)

## Collecting metrics

Common ways:

* As plain text: send a GET request to `<ip>:<port>/metrics`, for example, using CURL
or a web browser
* Set up Prometheus:
  1) Configure the instance to monitor the endpoint metrics (see [here](https://prometheus.io/docs/prometheus/latest/getting_started/#configure-prometheus-to-monitor-the-sample-targets))
  2) Use [the graph interface](https://prometheus.io/docs/prometheus/latest/getting_started/#using-the-graphing-interface)

## License

Apache 2.0
