# AdGuard VPN Endpoint Binary

[![AdGuardVPN.com](https://img.shields.io/badge/AdGuardVPN.com-Visit-007BFF)](https://adguard-vpn.com/)

A standalone application that allows any user to easily set up their own VPN server.

---

## Configuration

- **Configuration via TOML Files:** The VPN endpoint binary utilizes TOML formatted
  files for configuration.
  The following files are used:
    - Library Settings File: This file contains the configuration of the underlying library and
      reflects the `Settings` struct.
    - TLS Hosts Settings File: This file defines the TLS hosts that the endpoint can represent to
      the client side.
      Different types of hosts are available, each serving a specific purpose.
      It reflects the `TlsHostsSettings` struct.

- **Additional Configuration Requirements:** In addition to the TOML files, two additional items are
  required for configuration:
    - Credentials File: This file contains user authentication data.
    - Certificate Files: These files correspond to the TLS hosts defined in the TLS hosts settings.

- **Setup Wizard Tool:** A setup wizard tool is provided within the repository, located in a
  separate directory. This tool simplifies the process of generating the required settings and files
  mentioned above. Refer to the [usage instructions](../README.md#usage) for quick setup
  instructions.

- **Command Line Configuration:** The VPN endpoint binary supports additional configuration options
  through command line arguments. Users can customize the behavior of the endpoint according to
  their specific requirements. To view the available options, run the following command in the
  Terminal:
   ```shell
   vpn_endpoint -h
   ```

---

## Additional Features

### Testing Network Bandwidth

The endpoint provides a method to test the network bandwidth between the client and
the endpoint machines.

To conduct a download test, use the following command in the Terminal:

```shell
curl https://username:password@vpn.endpoint:443/100mb.bin \
  -k --resolve vpn.endpoint:443:127.0.0.1 -o /dev/null
```

Replace `vpn.endpoint` with the actual VPN endpoint hostname, and modify `username` and `password`
according to your settings.
Adjust `127.0.0.1` and `443` to the IP address and port number of the endpoint.
The `100mb.bin` represents the maximum available size of the file (1 to 100).

To conduct an upload test, use the following command in the Terminal:

```shell
curl https://username:password@vpn.endpoint:443/upload.html \
  -k --resolve vpn.endpoint:443:127.0.0.1 -F 'data=@./100mb.bin' -o /dev/null
```

Replace `vpn.endpoint`, `username`, `password`, `127.0.0.1`, and `443` as mentioned above.
The `./100mb.bin` should point to the file to be transferred, with a maximum file size of 120MB.

---

## Dynamic Reloading of TLS Host Settings

The endpoint supports dynamic reloading of TLS host settings.
When the SIGHUP signal is sent to the endpoint process,
it will update and reload the TLS host settings on-the-fly without requiring a restart
of the binary.
