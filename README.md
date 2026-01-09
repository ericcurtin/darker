# Darker

A Docker-like container runtime for macOS using native Darwin APIs.

Darker provides a familiar Docker-compatible CLI for running containers on macOS without requiring a Linux VM.

## Features

- **Docker-compatible CLI** - Familiar commands like `run`, `build`, `pull`, `exec`
- **OCI image support** - Pull and run images from Docker Hub and other OCI registries
- **Native macOS** - Runs directly on Darwin
- **Rootless by default** - Runs without root privileges
- **Chroot isolation** - Full filesystem isolation
- **Volume mounts** - Bind mount host directories into containers

## Installation

### From source

```bash
git clone https://github.com/ericcurtin/darker
cd darker
cargo build --release
sudo cp target/release/darker /usr/local/bin/
```

## Quick Start

```bash
# Run a simple command
darker run scratch echo "Hello from Darker!"

# Interactive shell
darker run -it --rm scratch sh

# Run with volume mount
darker run -v /path/on/host:/path/in/container scratch ls /path/in/container

# List images
darker images

# List containers
darker ps -a
```

## Commands

| Command | Description |
|---------|-------------|
| `run` | Create and run a container |
| `exec` | Execute a command in a running container |
| `build` | Build an image from a Dockerfile |
| `pull` | Pull an image from a registry |
| `push` | Push an image to a registry |
| `images` | List images |
| `ps` | List containers |
| `start` | Start stopped containers |
| `stop` | Stop running containers |
| `restart` | Restart containers |
| `rm` | Remove containers |
| `rmi` | Remove images |
| `logs` | Fetch container logs |
| `inspect` | Return low-level information on containers or images |
| `tag` | Create a tag for an image |
| `volume` | Manage volumes |
| `network` | Manage networks |
| `system` | Manage Darker (prune, info) |
| `attach` | Attach to a running container |

## Usage Examples

### Building images

```bash
# Build from Dockerfile in current directory
darker build -t my-app .

# Build with custom Dockerfile
darker build -f Dockerfile.prod -t my-app:prod .
```

### Managing containers

```bash
# List running containers
darker ps

# List all containers
darker ps -a

# Stop a container
darker stop my-container

# Remove a container
darker rm my-container

# View logs
darker logs my-container
darker logs -f my-container  # Follow logs
```

### Volumes

```bash
# Create a volume
darker volume create my-data

# List volumes
darker volume ls

# Mount a volume
darker run -v my-data:/data scratch ls /data

# Bind mount a host directory
darker run -v /Users/me/code:/app scratch ls /app
```

## Storage

Darker stores data in `~/.darker/`:

```
~/.darker/
├── containers/     # Container rootfs and metadata
├── images/         # Image layers and configs
├── volumes/        # Named volumes
└── tmp/            # Temporary files
```

## Environment Variables

- `DARKER_ROOT` - Override the default storage location
- `DARKER_LOG` - Set log level (trace, debug, info, warn, error)

## Comparison with Docker

| Feature | Darker | Docker |
|---------|--------|----------------|
| Linux VM | No | Yes |
| Native performance | Yes | VM overhead |

