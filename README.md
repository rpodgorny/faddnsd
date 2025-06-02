# FADDNSD - Freakin' Awesome Dynamic DNS Server

A dead simple yet functional implementation of dynamic DNS server written in Rust.

## Features

- **Dynamic DNS Updates**: Update DNS records via HTTP requests
- **DNSSEC Support**: Automatic zone signing with `dnssec-signzone`
- **Zone Validation**: Built-in zone file validation with `named-checkzone`
- **Automatic Reloading**: Triggers DNS server reload via `rndc`
- **IPv4/IPv6 Support**: Listens on both IPv4 and IPv6 interfaces
- **Web Interface**: Simple HTTP API for record updates
- **Background Processing**: Automatic periodic zone updates every 30 seconds

## Installation

### Prerequisites

- Rust 1.75+ (for building from source)
- BIND DNS utilities (`named-checkzone`, `rndc`, `dnssec-signzone`)

### Building from Source

```bash
git clone <repository-url>
cd faddnsd
cargo build --release
```

## Usage

### Basic Usage

```bash
faddnsd <zone> <zone_file> [serial_file] [options]
```

### Command Line Options

- `zone`: DNS zone name (e.g., `example.com`)
- `zone_file`: Path to the DNS zone file
- `serial_file`: Path to the serial file (optional, defaults to zone_file)
- `--port, -p`: HTTP server port (default: 8765)
- `--no-zone-reload`: Skip `rndc reload` after updates
- `--no-zone-sign`: Skip DNSSEC zone signing
- `--debug`: Enable debug logging

### Examples

```bash
# Basic usage
faddnsd example.com /etc/bind/zones/example.com.zone

# Custom port and serial file
faddnsd example.com /etc/bind/zones/example.com.zone /etc/bind/zones/example.com.serial --port 9000

# Skip DNSSEC signing and reload
faddnsd example.com /etc/bind/zones/example.com.zone --no-zone-sign --no-zone-reload
```

## API Endpoints

The server provides HTTP endpoints for updating DNS records:

- `GET /`: Web interface
- `POST /update`: Update DNS records
- `GET /status`: Server status information

## Development

### Running Tests

```bash
just test
```

### Code Coverage

```bash
just coverage
```

### Building Container Image

```bash
just podman-build
```

## Configuration

### Systemd Service

A systemd service file is provided in `etc/faddnsd.service`. Install it with:

```bash
sudo cp etc/faddnsd.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable faddnsd
sudo systemctl start faddnsd
```

## Dependencies

- **tokio**: Async runtime
- **axum**: Web framework
- **clap**: Command line parsing
- **serde**: Serialization
- **tracing**: Logging
- **regex**: Pattern matching

## License

TODO - License information to be added
