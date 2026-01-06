# mrpir - Motion-Activated Screen Control

A Rust daemon that monitors a PIR (Passive Infrared) motion sensor on a Raspberry Pi and:

- **Publishes motion events to MQTT** with Home Assistant auto-discovery
- **Controls screen brightness** based on motion detection
- **Supports night mode** with sunrise/sunset awareness
- **Integrates with systemd** for reliable service management

This is a complete Rust rewrite of [mrpir](https://github.com/MichaelRegan/mrpir) (Python), designed for improved performance, reliability, and modern Rust best practices.

## Features

- 🚀 **Pure Rust** - No Python runtime required, minimal dependencies
- 📡 **MQTT Integration** - Publishes motion state to any MQTT broker
- 🏠 **Home Assistant Discovery** - Automatic entity configuration
- 🌙 **Night Mode** - Automatically turn off screen during night hours
- 🌅 **Sunrise/Sunset Awareness** - Uses astronomical calculations for sun times
- 💡 **Screen Control** - Brightness control via sysfs or Wayland
- 🔧 **Flexible Configuration** - TOML config files with environment variable overrides
- 🐧 **Systemd Integration** - Watchdog support and proper service management

## Installation

### Prerequisites

- Raspberry Pi (or compatible SBC) running Linux
- PIR Motion Sensor connected to a GPIO pin
- Rust toolchain (1.75+) for building

### From Source

```bash
# Clone the repository
git clone https://github.com/MichaelRegan/rust-mrpir.git
cd rust-mrpir

# Build release binary
cargo build --release

# Install
sudo cp target/release/mrpir /usr/local/bin/

# Create configuration directory
sudo mkdir -p /etc/mrpir
sudo cp config.example.toml /etc/mrpir/config.toml

# Edit configuration
sudo nano /etc/mrpir/config.toml

# Install systemd service
sudo cp mrpir.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable mrpir
sudo systemctl start mrpir
```

### Cross-Compilation for Raspberry Pi

```bash
# Add ARM target
rustup target add armv7-unknown-linux-gnueabihf

# Build for Pi
cargo build --release --target armv7-unknown-linux-gnueabihf

# Copy to Pi
scp target/armv7-unknown-linux-gnueabihf/release/mrpir pi@raspberrypi:/tmp/
```

## Configuration

mrpir uses layered configuration with the following priority (highest first):

1. Environment variables (`MRPIR_*`)
2. Local `config.toml`
3. User config `~/.config/mrpir/config.toml`
4. System config `/etc/mrpir/config.toml`
5. Built-in defaults

### Example Configuration

```toml
device_name = "bedroom"

[sensor]
gpio_pin = 17
no_motion_delay_secs = 5

[mqtt]
enabled = true
host = "192.168.1.100"
port = 1883
username = "iot"
password = "secret"
ha_discovery = true

[screen]
enabled = true
method = "brightness"
dim_brightness = 0
bright_brightness = 230
motion_timeout_secs = 30

[night_mode]
enabled = true
use_sun_times = true
sundown_delay_secs = 3600

[location]
latitude = 40.7128
longitude = -74.0060
```

### Environment Variable Overrides

Environment variables use the `MRPIR_` prefix with underscores for nesting:

```bash
export MRPIR_DEVICE_NAME=livingroom
export MRPIR_MQTT_HOST=192.168.1.100
export MRPIR_MQTT_USERNAME=iot
export MRPIR_SENSOR_GPIO_PIN=17
```

### Configuration Reference

#### Sensor Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `sensor.gpio_pin` | 17 | GPIO pin number (BCM) for PIR sensor |
| `sensor.no_motion_delay_secs` | 5 | Delay before reporting motion cleared |
| `sensor.poll_interval_ms` | 100 | Sensor polling interval |

#### MQTT Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `mqtt.enabled` | true | Enable MQTT publishing |
| `mqtt.host` | localhost | MQTT broker hostname |
| `mqtt.port` | 1883 | MQTT broker port |
| `mqtt.username` | - | MQTT username (optional) |
| `mqtt.password` | - | MQTT password (optional) |
| `mqtt.ha_discovery` | true | Enable Home Assistant discovery |
| `mqtt.ha_discovery_prefix` | homeassistant | HA discovery topic prefix |

#### Screen Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `screen.enabled` | false | Enable screen control |
| `screen.method` | none | Control method: `none`, `brightness`, `wayland` |
| `screen.dim_brightness` | 0 | Brightness when dimmed (0-255) |
| `screen.bright_brightness` | 230 | Brightness when active (0-255) |
| `screen.motion_timeout_secs` | 30 | Seconds before dimming |

#### Night Mode Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `night_mode.enabled` | false | Enable night mode |
| `night_mode.use_sun_times` | false | Use sunrise/sunset |
| `night_mode.start_hour` | 22 | Night start (24h format) |
| `night_mode.end_hour` | 6 | Night end (24h format) |

#### Location Settings (for sunrise/sunset)

| Setting | Description |
|---------|-------------|
| `location.latitude` | Latitude in decimal degrees |
| `location.longitude` | Longitude in decimal degrees |

## Usage

### Running Directly

```bash
# With default config locations
mrpir

# With custom config
MRPIR_DEVICE_NAME=test mrpir

# With debug logging
RUST_LOG=mrpir=debug mrpir
```

### Systemd Service

```bash
# Start service
sudo systemctl start mrpir

# Check status
sudo systemctl status mrpir

# View logs
journalctl -u mrpir -f

# Restart after config change
sudo systemctl restart mrpir
```

### User Service (for screen control)

For Wayland screen control, run as a user service:

```bash
# Copy service file
mkdir -p ~/.config/systemd/user/
cp mrpir.service ~/.config/systemd/user/

# Edit for user mode (remove SupplementaryGroups, adjust paths)
nano ~/.config/systemd/user/mrpir.service

# Enable lingering for boot start
sudo loginctl enable-linger $USER

# Start
systemctl --user enable mrpir
systemctl --user start mrpir
```

## Home Assistant Integration

With MQTT discovery enabled, mrpir automatically creates a binary sensor in Home Assistant:

```yaml
# Example automation
automation:
  - alias: "Turn on lights when motion detected"
    trigger:
      - platform: state
        entity_id: binary_sensor.bedroom_motion
        to: "on"
    action:
      - service: light.turn_on
        target:
          entity_id: light.bedroom
```

## Building Features

mrpir supports optional features:

```bash
# Default (brightness control via sysfs)
cargo build --release

# With Wayland support
cargo build --release --features wayland-control

# Minimal (no screen control)
cargo build --release --no-default-features
```

## Architecture

```
src/
├── main.rs           # Entry point, main loop, signal handling
├── config.rs         # Configuration management (figment)
├── error.rs          # Custom error types (thiserror)
├── mqtt/
│   ├── mod.rs        # Module exports
│   ├── client.rs     # MQTT client (rumqttc)
│   └── discovery.rs  # Home Assistant discovery payloads
├── screen/
│   ├── mod.rs        # Screen controller trait
│   ├── brightness_ctrl.rs  # Sysfs brightness control
│   └── wayland.rs    # Wayland wlr-output-power
├── sensor/
│   └── mod.rs        # PIR sensor (rppal GPIO)
└── time_events.rs    # Night mode, sunrise/sunset
```

## Troubleshooting

### GPIO Permission Denied

```bash
# Add user to gpio group
sudo usermod -aG gpio $USER

# Or run as root (not recommended)
sudo mrpir
```

### MQTT Connection Failed

- Verify broker is running: `mosquitto_sub -h localhost -t '#'`
- Check firewall: `sudo ufw allow 1883`
- Verify credentials in config

### Screen Control Not Working

- **brightness method**: Check `/sys/class/backlight/` for device
- **wayland method**: Ensure `WAYLAND_DISPLAY` is set, install `wlr-randr`

### Logs

```bash
# Systemd service logs
journalctl -u mrpir -f

# Debug logging
RUST_LOG=mrpir=debug mrpir
```

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Run `cargo fmt` and `cargo clippy`
4. Submit a pull request

## License

MIT License - see [LICENSE](LICENSE) file.

## Acknowledgements

- [rppal](https://github.com/golemparts/rppal) - Raspberry Pi GPIO
- [rumqttc](https://github.com/bytebeamio/rumqtt) - MQTT client
- [figment](https://github.com/SergioBenitez/Figment) - Configuration
- [astro](https://github.com/saurvs/astro) - Astronomical calculations
