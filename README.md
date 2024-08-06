# epimetheus

[![uses nix](https://img.shields.io/badge/uses-nix-%237EBAE4)](https://nixos.org/)

`epimetheus` is a Rust application designed to read metric data from various file formats (JSON, YAML, CSV) and expose them as Prometheus metrics. It's built with flexibility in mind, allowing you to easily monitor and export metrics from different data sources.

It is named for [Epimetheus, brother of Prometheus](https://en.wikipedia.org/wiki/Epimetheus), who is sometimes referred to as `afterthought` or `hindsight` in Greek Mythology - representing the fact that this tool can come in handy when metrics might be a requirement that was not initially planned for!

## Features

- Supports multiple file formats: JSON, YAML, and CSV (loading from local files and remote URLs!)
- Configurable metric prefix for easy grouping and identification
- Ability to ignore specific keys in the input data
- Periodic updates of metrics at a configurable interval
- Exposes metrics in Prometheus format via HTTP endpoint

## Installation

To build and run epimetheus, you need to have [nix](https://nixos.org/) + [direnv](https://github.com/direnv/direnv), or Rust and Cargo installed on your system.

Clone the repository:

```bash
git clone https://github.com/jpetrucciani/epimetheus.git
cd epimetheus
```

Build the project:

```bash
cargo build --release
```

The compiled binary will be available in the `target/release` directory.

## Usage

You can run epimetheus with various command-line options or environment variables:

```bash
./epimetheus [OPTIONS]
```

### Options

| Flag                            | Environment Variable | Default    | Description                                                   |
| ------------------------------- | -------------------- | ---------- | ------------------------------------------------------------- |
| `--listen-addr <IP>`            | `EPI_IP`             | `0.0.0.0`  | IP address to listen on                                       |
| `--port <PORT>`                 | `EPI_PORT`           | `8080`     | Port to listen on                                             |
| `--files <FILE1,FILE2,...>`     | `EPI_FILES`          | (Required) | Comma-separated list of files (or urls!) to read metrics from |
| `--ignore-keys <KEY1,KEY2,...>` | `EPI_IGNORE_KEYS`    |            | Comma-separated list of keys to ignore in the input data      |
| `--interval <SECONDS>`          | `EPI_INTERVAL`       | `60`       | Update interval in seconds                                    |
| `--log-format <LOG_FORMAT>`     | `EPI_LOG_FORMAT`     | `json`     | Log message format. Supports `json` and `term`                |
| `--log-level <LOG_LEVEL>`       | `EPI_LOG_LEVEL`      | `info`     | `critical`, `error`, `warning`, `info`, `debug`               |
| `--metric-prefix <PREFIX>`      | `EPI_METRIC_PREFIX`  | `""`       | Prefix to add to all metric names                             |

### Example

```bash
./epimetheus --files data1.json,data2.yaml,data3.csv --ignore-keys timestamp,version --interval 30 --metric-prefix "meme_"
```

This command will:

- Read metrics from `data1.json`, `data2.yaml`, and `data3.csv`
- Ignore the keys "timestamp" and "version" in the input data
- Update metrics every 30 seconds
- Add the prefix `meme_` to all metric names

## Accessing/Scraping Metrics

Once the application is running, you can access the metrics by sending a GET request to the `/metrics` endpoint:

```bash
http://localhost:8080/metrics
```

This will return the metrics in Prometheus format.

## Demo

![epimetheus_demo](https://cobi.dev/static/img/github/gif/epimetheus-0.1.0.gif)
