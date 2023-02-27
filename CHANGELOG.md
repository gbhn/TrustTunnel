# CHANGELOG

* Added support for configuring the library with multiple TLS certificates.
  API changes:
    * `settings::Settings::tunnel_tls_host_info` is renamed to `settings::Settings::tunnel_tls_hosts` and is now a vector of hosts
    * `settings::Settings::ping_tls_host_info` is renamed to `settings::Settings::ping_tls_hosts` and is now a vector of hosts
    * `settings::Settings::speed_tls_host_info` is renamed to `settings::Settings::speed_tls_hosts` and is now a vector of hosts
    * `settings::ReverseProxySettings::tls_host_info` is renamed to `settings::ReverseProxySettings::tls_hosts` and is now a vector of hosts

## 0.9.24

* Added speedtest support

## 0.9.13

* Test changelog entry please ignore
