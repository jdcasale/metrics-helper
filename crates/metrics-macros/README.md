# metrics-helper-macros

[![crates.io](https://img.shields.io/crates/v/metrics-helper-macros.svg)](https://crates.io/crates/metrics-helper-macros)
[![docs.rs](https://docs.rs/metrics-helper-macros/badge.svg)](https://docs.rs/metrics-helper-macros)

Proc-macros for idiomatic Prometheus metrics instrumentation in Rust.

## Features

- **`#[instrument_metrics]`** - Attribute macro for automatic function instrumentation
- **Counters** - Count function invocations
- **Histograms** - Measure function duration
- **Error counters** - Track errors for functions returning `Result`
- **Labels** - Attach static or dynamic labels from function parameters
- **Zero-cost** - All instrumentation compiles away when `metrics` feature is disabled

## Installation

```toml
[dependencies]
metrics-helper-macros = "0.1"
metrics = "0.24"

[features]
metrics = ["metrics-helper-macros/metrics"]
```

## Usage

### Basic Example

```rust
use metrics_helper_macros::instrument_metrics;

#[instrument_metrics(
    counter = "db_queries_total",
    histogram = "db_query_duration_seconds",
    error_counter = "db_query_errors_total",
)]
async fn query_database() -> Result<Data, DbError> {
    // Your code here...
}
```

### With Labels

Labels can be **static** (fixed values) or **dynamic** (captured from function parameters):

```rust
#[instrument_metrics(
    counter = "http_requests_total",
    histogram = "http_request_duration_seconds",
    labels(
        service = "api",      // static label
        method,               // dynamic: captured from function param
        endpoint,             // dynamic: captured from function param
    ),
)]
async fn handle_request(method: &str, endpoint: &str, body: Bytes) -> Response {
    // method and endpoint values are captured as label values
}
```

This produces metrics like:
```
http_requests_total{service="api", method="GET", endpoint="/users"} 1
http_request_duration_seconds{service="api", method="GET", endpoint="/users"} 0.023
```

### Attributes

| Attribute | Description |
|-----------|-------------|
| `counter` | Counter to increment on each call |
| `histogram` | Histogram to record call duration (in seconds) |
| `error_counter` | Counter to increment when function returns `Err` |
| `labels(...)` | Labels to attach to all metrics |

### Label Syntax

- **Static**: `key = "value"` - Fixed string value
- **Dynamic**: `key` - Captures the value of a function parameter with the same name (must implement `Display`)

## Feature Gating

All metric recording is wrapped in `#[cfg(feature = "metrics")]`:

```rust
// When metrics feature is disabled, this compiles to just:
async fn my_function() -> Result<(), Error> {
    // original function body
}

// When metrics feature is enabled, it includes instrumentation:
async fn my_function() -> Result<(), Error> {
    metrics::counter!("calls_total").increment(1);
    let start = std::time::Instant::now();
    let result = async { /* original body */ }.await;
    metrics::histogram!("duration_seconds").record(start.elapsed().as_secs_f64());
    if result.is_err() {
        metrics::counter!("errors_total").increment(1);
    }
    result
}
```

## Requirements

- Functions using dynamic labels must have parameters that implement `Display`
- The `metrics` crate must be available in your dependency tree
- For `error_counter` to work, the function must return a `Result` type

## License

MIT
