use axum::{routing::get, Router};
use clap::Parser;
use csv::ReaderBuilder;
use prometheus::{Encoder, Gauge, IntCounter, IntGauge, Registry, TextEncoder};
use reqwest::Client;
use serde_json::Value;
use serde_yaml;
use slog::{o, Drain, Level, Logger};
use slog_term;
use std::str::FromStr;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::{fs, sync::RwLock, time};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, env = "EPI_IP", default_value = "0.0.0.0")]
    listen_addr: String,

    #[arg(long, env = "EPI_PORT", default_value_t = 8080)]
    port: u16,

    #[arg(long, env = "EPI_FILES", required = true, num_args = 1.., value_delimiter = ',')]
    files: Vec<String>,

    #[arg(long, env = "EPI_IGNORE_KEYS", num_args = 1.., value_delimiter = ',')]
    ignore_keys: Vec<String>,

    #[arg(long, env = "EPI_INTERVAL", default_value_t = 60)]
    interval: u64,

    #[arg(long, env = "EPI_METRIC_PREFIX", default_value = "")]
    metric_prefix: String,

    #[arg(long, env = "EPI_LOG_FORMAT", default_value = "json")]
    log_format: String,

    #[arg(long, env = "EPI_LOG_LEVEL", default_value = "info")]
    log_level: String,
}

struct InternalMetrics {
    sources_total: IntGauge,
    source_reads_total: IntCounter,
    source_read_failures_total: IntCounter,
    metrics_total: IntGauge,
}

impl InternalMetrics {
    fn new(registry: &Registry) -> Self {
        let sources_total = IntGauge::new(
            "epimetheus_sources_total",
            "Total number of file/url sources",
        )
        .unwrap();
        let source_reads_total = IntCounter::new(
            "epimetheus_source_reads_total",
            "Total number of source read attempts",
        )
        .unwrap();
        let source_read_failures_total = IntCounter::new(
            "epimetheus_source_read_failures_total",
            "Total number of source read failures",
        )
        .unwrap();
        let metrics_total = IntGauge::new(
            "epimetheus_metrics_total",
            "Total number of metrics being tracked",
        )
        .unwrap();

        registry.register(Box::new(sources_total.clone())).unwrap();
        registry
            .register(Box::new(source_reads_total.clone()))
            .unwrap();
        registry
            .register(Box::new(source_read_failures_total.clone()))
            .unwrap();
        registry.register(Box::new(metrics_total.clone())).unwrap();

        InternalMetrics {
            sources_total,
            source_reads_total,
            source_read_failures_total,
            metrics_total,
        }
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let log_level = Level::from_str(&args.log_level).unwrap_or(Level::Info);
    let log = setup_logger(&args.log_format, log_level);

    slog::info!(log, "Starting epimetheus"; "listen_addr" => &args.listen_addr, "port" => args.port);

    let registry = Registry::new();
    let metrics = Arc::new(RwLock::new(HashMap::new()));
    let internal_metrics = Arc::new(InternalMetrics::new(&registry));

    internal_metrics.sources_total.set(args.files.len() as i64);

    let update_log = log.clone();
    tokio::spawn(update_metrics(
        args.files.clone(),
        args.ignore_keys.clone(),
        args.interval,
        Arc::clone(&metrics),
        registry.clone(),
        args.metric_prefix.clone(),
        update_log,
        Arc::clone(&internal_metrics),
    ));

    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state((
            registry,
            metrics,
            log.clone(),
            Arc::clone(&internal_metrics),
        ));

    let addr = format!("{}:{}", args.listen_addr, args.port);
    slog::info!(log, "Listening"; "address" => &addr);
    axum::Server::bind(&addr.parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

fn setup_logger(format: &str, level: Level) -> Logger {
    let drain = match format {
        "json" => {
            let drain = slog_json::Json::new(std::io::stdout())
                .add_default_keys()
                .build()
                .fuse();
            slog_async::Async::new(drain).build().fuse()
        }
        "term" => {
            let decorator = slog_term::TermDecorator::new().build();
            let drain = slog_term::FullFormat::new(decorator).build().fuse();
            slog_async::Async::new(drain).build().fuse()
        }
        _ => {
            eprintln!("Invalid log format specified. Defaulting to JSON.");
            let drain = slog_json::Json::new(std::io::stdout())
                .add_default_keys()
                .build()
                .fuse();
            slog_async::Async::new(drain).build().fuse()
        }
    };

    let drain = drain.filter_level(level).fuse();
    Logger::root(drain, o!("version" => env!("CARGO_PKG_VERSION")))
}

async fn update_metrics(
    files: Vec<String>,
    ignore_keys: Vec<String>,
    interval: u64,
    metrics: Arc<RwLock<HashMap<String, Gauge>>>,
    registry: Registry,
    metric_prefix: String,
    log: Logger,
    internal_metrics: Arc<InternalMetrics>,
) {
    let mut interval = time::interval(Duration::from_secs(interval));
    let client = Client::new();

    loop {
        interval.tick().await;

        let mut metric_count: i64 = 0;

        for file in &files {
            slog::debug!(log, "Processing file"; "file" => file);
            internal_metrics.source_reads_total.inc();

            let (contents, file_type) = if file.starts_with("http://")
                || file.starts_with("https://")
            {
                match fetch_url(&client, file, &log).await {
                    Ok((content, detected_type)) => (content, detected_type),
                    Err(e) => {
                        slog::error!(log, "Error fetching URL"; "url" => file, "error" => %e);
                        internal_metrics.source_read_failures_total.inc();
                        continue;
                    }
                }
            } else {
                match fs::read_to_string(file).await {
                    Ok(contents) => {
                        let path = PathBuf::from(file);
                        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        slog::debug!(log, "Local file read successfully"; "file" => file, "extension" => extension);
                        (contents, extension.to_string())
                    }
                    Err(e) => {
                        slog::error!(log, "Error reading file"; "file" => file, "error" => %e);
                        internal_metrics.source_read_failures_total.inc();
                        continue;
                    }
                }
            };

            slog::debug!(log, "Processing content"; "file" => file, "file_type" => &file_type);
            match file_type.as_str() {
                "json" => {
                    metric_count += process_json(
                        &contents,
                        &ignore_keys,
                        &metrics,
                        &registry,
                        &metric_prefix,
                        &log,
                        &internal_metrics,
                    )
                    .await
                }
                "yaml" | "yml" => {
                    metric_count += process_yaml(
                        &contents,
                        &ignore_keys,
                        &metrics,
                        &registry,
                        &metric_prefix,
                        &log,
                        &internal_metrics,
                    )
                    .await
                }
                "csv" => {
                    metric_count += process_csv(
                        &contents,
                        &ignore_keys,
                        &metrics,
                        &registry,
                        &metric_prefix,
                        &log,
                        &internal_metrics,
                    )
                    .await
                }
                _ => {
                    slog::warn!(log, "Unsupported file format"; "file" => file, "file_type" => file_type)
                }
            }
        }
        internal_metrics.metrics_total.set(metric_count);
    }
}

async fn fetch_url(
    client: &Client,
    url: &str,
    log: &Logger,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    slog::debug!(log, "Fetching URL"; "url" => url);
    let response = client.get(url).send().await?;

    let file_type = detect_file_type_from_headers(response.headers());
    slog::debug!(log, "Detected file type from headers"; "url" => url, "file_type" => &file_type);
    let content = response.text().await?;

    Ok((content, file_type))
}

const CSV_TYPES: [&str; 1] = ["text/csv"];
const JSON_TYPES: [&str; 1] = ["application/json"];
const YAML_TYPES: [&str; 3] = ["application/yaml", "application/x-yaml", "text/x-yaml"];

fn detect_file_type_from_headers(headers: &reqwest::header::HeaderMap) -> String {
    if let Some(content_type) = headers.get(reqwest::header::CONTENT_TYPE) {
        let content_type = content_type.to_str().unwrap_or("").to_lowercase();
        if JSON_TYPES.iter().any(|t| content_type.contains(t)) {
            return "json".to_string();
        } else if YAML_TYPES.iter().any(|t| content_type.contains(t)) {
            return "yaml".to_string();
        } else if CSV_TYPES.iter().any(|t| content_type.contains(t)) {
            return "csv".to_string();
        }
    }

    "unknown".to_string()
}

async fn process_json(
    contents: &str,
    ignore_keys: &[String],
    metrics: &Arc<RwLock<HashMap<String, Gauge>>>,
    registry: &Registry,
    metric_prefix: &str,
    log: &Logger,
    internal_metrics: &Arc<InternalMetrics>,
) -> i64 {
    slog::debug!(log, "Processing JSON content");
    if let Ok(json) = serde_json::from_str::<Value>(contents) {
        let flattened = flatten_json(&json);
        let metric_count = update_metrics_from_map(
            &flattened,
            ignore_keys,
            metrics,
            registry,
            metric_prefix,
            log,
            internal_metrics,
        )
        .await;
        return metric_count;
    } else {
        slog::error!(log, "Failed to parse JSON content");
    }
    return 0;
}

async fn process_yaml(
    contents: &str,
    ignore_keys: &[String],
    metrics: &Arc<RwLock<HashMap<String, Gauge>>>,
    registry: &Registry,
    metric_prefix: &str,
    log: &Logger,
    internal_metrics: &Arc<InternalMetrics>,
) -> i64 {
    slog::debug!(log, "Processing YAML content");
    if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(contents) {
        if let Ok(json) = serde_json::to_value(yaml) {
            let flattened = flatten_json(&json);
            let metric_count = update_metrics_from_map(
                &flattened,
                ignore_keys,
                metrics,
                registry,
                metric_prefix,
                log,
                internal_metrics,
            )
            .await;

            return metric_count;
        } else {
            slog::error!(log, "Failed to convert YAML to JSON");
        }
    } else {
        slog::error!(log, "Failed to parse YAML content");
    }
    return 0;
}

async fn process_csv(
    contents: &str,
    ignore_keys: &[String],
    metrics: &Arc<RwLock<HashMap<String, Gauge>>>,
    registry: &Registry,
    metric_prefix: &str,
    log: &Logger,
    internal_metrics: &Arc<InternalMetrics>,
) -> i64 {
    slog::debug!(log, "Processing CSV content");
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(contents.as_bytes());

    let headers = match reader.headers() {
        Ok(headers) => headers.clone(),
        Err(_) => {
            slog::error!(log, "Failed to read CSV headers");
            return 0;
        }
    };

    let first_row = match reader.records().next() {
        Some(Ok(row)) => row,
        _ => {
            slog::error!(log, "Failed to read first CSV row");
            return 0;
        }
    };

    let obj: HashMap<String, Value> = headers
        .iter()
        .zip(first_row.iter())
        .filter_map(|(header, value)| {
            value.parse::<f64>().ok().map(|num| {
                (
                    header.to_string(),
                    Value::Number(serde_json::Number::from_f64(num).unwrap()),
                )
            })
        })
        .collect();

    let metric_count = update_metrics_from_map(
        &obj,
        ignore_keys,
        metrics,
        registry,
        metric_prefix,
        log,
        internal_metrics,
    )
    .await;

    return metric_count;
}

fn flatten_json(value: &Value) -> HashMap<String, Value> {
    let mut map = HashMap::new();
    flatten_json_inner(value, String::new(), &mut map);
    map
}

fn flatten_json_inner(value: &Value, prefix: String, map: &mut HashMap<String, Value>) {
    match value {
        Value::Object(obj) => {
            for (k, v) in obj {
                let new_prefix = if prefix.is_empty() {
                    k.to_string()
                } else {
                    format!("{}__{}", prefix, k)
                };
                flatten_json_inner(v, new_prefix, map);
            }
        }
        Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                let new_prefix = format!("{}__{}", prefix, i);
                flatten_json_inner(v, new_prefix, map);
            }
        }
        _ => {
            map.insert(prefix, value.clone());
        }
    }
}

async fn update_metrics_from_map(
    obj: &HashMap<String, Value>,
    ignore_keys: &[String],
    metrics: &Arc<RwLock<HashMap<String, Gauge>>>,
    registry: &Registry,
    metric_prefix: &str,
    log: &Logger,
    _internal_metrics: &Arc<InternalMetrics>,
) -> i64 {
    let mut metrics_count: i64 = 0;
    for (key, value) in obj {
        if !ignore_keys.contains(key) {
            let metric_name = format!("{}{}", metric_prefix, key);
            let mut metrics = metrics.write().await;

            match value {
                Value::Number(num) => {
                    if let Some(n) = num.as_f64() {
                        let gauge = metrics.entry(metric_name.clone()).or_insert_with(|| {
                            let gauge = Gauge::new(metric_name.clone(), key.clone()).unwrap();
                            registry.register(Box::new(gauge.clone())).unwrap();
                            gauge
                        });
                        gauge.set(n);
                        metrics_count += 1;
                        slog::debug!(log, "Updated numeric metric"; "metric" => &metric_name, "value" => n);
                    }
                }
                Value::String(s) => {
                    if let Ok(n) = s.parse::<f64>() {
                        let gauge = metrics.entry(metric_name.clone()).or_insert_with(|| {
                            let gauge = Gauge::new(metric_name.clone(), key.clone()).unwrap();
                            registry.register(Box::new(gauge.clone())).unwrap();
                            gauge
                        });
                        gauge.set(n);
                        metrics_count += 1;
                        slog::debug!(log, "Updated string metric"; "metric" => &metric_name, "value" => n);
                    } else {
                        slog::warn!(log, "Failed to parse string as number"; "metric" => &metric_name, "value" => s);
                    }
                }
                _ => {
                    slog::warn!(log, "Unsupported value type for metric"; "metric" => &metric_name);
                }
            }
        }
    }
    return metrics_count;
}

async fn metrics_handler(
    state: axum::extract::State<(
        Registry,
        Arc<RwLock<HashMap<String, Gauge>>>,
        Logger,
        Arc<InternalMetrics>,
    )>,
) -> String {
    slog::debug!(state.2, "Handling metrics request");
    let encoder = TextEncoder::new();
    let metric_families = state.0 .0.gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}
