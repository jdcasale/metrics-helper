//! Tests that verify metrics are actually recorded correctly.
//!
//! These tests use a debugging recorder to capture metrics and verify
//! that counters, histograms, and labels are properly recorded.

use metrics_helper_macros::instrument_metrics;
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use std::sync::Once;

static INIT: Once = Once::new();
static mut SNAPSHOTTER: Option<Snapshotter> = None;

/// Initialize the global test recorder (only once across all tests)
fn setup_recorder() -> &'static Snapshotter {
    INIT.call_once(|| {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        // SAFETY: This is only called once due to Once, and we only read after this
        unsafe {
            SNAPSHOTTER = Some(snapshotter);
        }
        recorder.install().expect("failed to install recorder");
    });
    // SAFETY: SNAPSHOTTER is initialized above and never modified after
    unsafe { SNAPSHOTTER.as_ref().unwrap() }
}

/// Helper to find a counter value by name
fn get_counter(snapshotter: &Snapshotter, name: &str) -> Option<u64> {
    for (key, _unit, _desc, value) in snapshotter.snapshot().into_vec() {
        if key.key().name() == name {
            if let DebugValue::Counter(count) = value {
                return Some(count);
            }
        }
    }
    None
}

/// Helper to find counter value with specific labels
fn get_counter_with_labels(
    snapshotter: &Snapshotter,
    name: &str,
    expected_labels: &[(&str, &str)],
) -> Option<u64> {
    for (key, _unit, _desc, value) in snapshotter.snapshot().into_vec() {
        if key.key().name() == name {
            let labels: Vec<_> = key.key().labels().collect();
            let matches = expected_labels.iter().all(|(k, v)| {
                labels.iter().any(|l| l.key() == *k && l.value() == *v)
            });
            if matches && labels.len() == expected_labels.len() {
                if let DebugValue::Counter(count) = value {
                    return Some(count);
                }
            }
        }
    }
    None
}

/// Helper to check if a histogram exists and has recordings
fn has_histogram(snapshotter: &Snapshotter, name: &str) -> bool {
    for (key, _unit, _desc, value) in snapshotter.snapshot().into_vec() {
        if key.key().name() == name {
            if let DebugValue::Histogram(values) = value {
                return !values.is_empty();
            }
        }
    }
    false
}

/// Helper to check histogram with specific labels
fn has_histogram_with_labels(
    snapshotter: &Snapshotter,
    name: &str,
    expected_labels: &[(&str, &str)],
) -> bool {
    for (key, _unit, _desc, value) in snapshotter.snapshot().into_vec() {
        if key.key().name() == name {
            let labels: Vec<_> = key.key().labels().collect();
            let matches = expected_labels.iter().all(|(k, v)| {
                labels.iter().any(|l| l.key() == *k && l.value() == *v)
            });
            if matches && labels.len() == expected_labels.len() {
                if let DebugValue::Histogram(values) = value {
                    return !values.is_empty();
                }
            }
        }
    }
    false
}

// ============================================================================
// Instrumented functions for testing
// ============================================================================

#[instrument_metrics(counter = "test_counter_total")]
fn simple_counter_fn() -> i32 {
    42
}

#[instrument_metrics(histogram = "test_histogram_seconds")]
fn simple_histogram_fn() {
    // Simulate some work
    std::thread::sleep(std::time::Duration::from_millis(1));
}

#[instrument_metrics(
    counter = "test_error_counter_total",
    error_counter = "test_errors_total"
)]
fn fn_returns_ok() -> Result<i32, &'static str> {
    Ok(42)
}

#[instrument_metrics(
    counter = "test_error_counter_total",
    error_counter = "test_errors_total"
)]
fn fn_returns_err() -> Result<i32, &'static str> {
    Err("failed")
}

#[instrument_metrics(
    counter = "test_static_labels_total",
    labels(service = "test-service", version = "v1")
)]
fn fn_with_static_labels() {}

#[instrument_metrics(
    counter = "test_dynamic_labels_total",
    labels(method)
)]
fn fn_with_dynamic_label(method: &str) {
    let _ = method;
}

#[instrument_metrics(
    counter = "test_mixed_labels_total",
    histogram = "test_mixed_labels_seconds",
    labels(service = "api", operation)
)]
fn fn_with_mixed_labels(operation: &str) {
    let _ = operation;
}

#[instrument_metrics(
    counter = "test_full_total",
    histogram = "test_full_seconds",
    error_counter = "test_full_errors_total",
    labels(service = "db", table, operation)
)]
fn full_instrumented(table: &str, operation: &str) -> Result<(), &'static str> {
    let _ = (table, operation);
    Ok(())
}

#[instrument_metrics(
    counter = "test_full_total",
    histogram = "test_full_seconds",
    error_counter = "test_full_errors_total",
    labels(service = "db", table, operation)
)]
fn full_instrumented_fails(table: &str, operation: &str) -> Result<(), &'static str> {
    let _ = (table, operation);
    Err("db error")
}

// ============================================================================
// Actual verification tests
// ============================================================================

#[test]
fn test_counter_increments() {
    let snapshotter = setup_recorder();

    let before = get_counter(snapshotter, "test_counter_total").unwrap_or(0);

    simple_counter_fn();
    simple_counter_fn();
    simple_counter_fn();

    let after = get_counter(snapshotter, "test_counter_total").unwrap_or(0);

    assert_eq!(after - before, 3, "Counter should have incremented by 3");
}

#[test]
fn test_histogram_records_duration() {
    let snapshotter = setup_recorder();

    simple_histogram_fn();

    // Note: DebuggingRecorder drains histogram samples on snapshot, so we verify
    // the histogram exists immediately after recording
    assert!(
        has_histogram(snapshotter, "test_histogram_seconds"),
        "Histogram should have recorded a duration"
    );
}

#[test]
fn test_error_counter_on_ok() {
    let snapshotter = setup_recorder();

    let before_calls = get_counter(snapshotter, "test_error_counter_total").unwrap_or(0);
    let before_errors = get_counter(snapshotter, "test_errors_total").unwrap_or(0);

    let result = fn_returns_ok();
    assert!(result.is_ok());

    let after_calls = get_counter(snapshotter, "test_error_counter_total").unwrap_or(0);
    let after_errors = get_counter(snapshotter, "test_errors_total").unwrap_or(0);

    assert_eq!(after_calls - before_calls, 1, "Call counter should increment");
    assert_eq!(after_errors - before_errors, 0, "Error counter should NOT increment on Ok");
}

#[test]
fn test_error_counter_on_err() {
    let snapshotter = setup_recorder();

    let before_calls = get_counter(snapshotter, "test_error_counter_total").unwrap_or(0);
    let before_errors = get_counter(snapshotter, "test_errors_total").unwrap_or(0);

    let result = fn_returns_err();
    assert!(result.is_err());

    let after_calls = get_counter(snapshotter, "test_error_counter_total").unwrap_or(0);
    let after_errors = get_counter(snapshotter, "test_errors_total").unwrap_or(0);

    assert_eq!(after_calls - before_calls, 1, "Call counter should increment");
    assert_eq!(after_errors - before_errors, 1, "Error counter SHOULD increment on Err");
}

#[test]
fn test_static_labels_attached() {
    let snapshotter = setup_recorder();

    fn_with_static_labels();

    let value = get_counter_with_labels(
        snapshotter,
        "test_static_labels_total",
        &[("service", "test-service"), ("version", "v1")],
    );

    assert!(
        value.is_some(),
        "Counter should exist with static labels service=test-service, version=v1"
    );
    assert!(value.unwrap() >= 1, "Counter should have at least 1 call");
}

#[test]
fn test_dynamic_labels_captured() {
    let snapshotter = setup_recorder();

    fn_with_dynamic_label("GET");
    fn_with_dynamic_label("POST");
    fn_with_dynamic_label("GET"); // Call GET again

    let get_count = get_counter_with_labels(
        snapshotter,
        "test_dynamic_labels_total",
        &[("method", "GET")],
    );

    let post_count = get_counter_with_labels(
        snapshotter,
        "test_dynamic_labels_total",
        &[("method", "POST")],
    );

    assert!(get_count.is_some(), "Counter with method=GET should exist");
    assert!(post_count.is_some(), "Counter with method=POST should exist");
    assert!(get_count.unwrap() >= 2, "GET should have at least 2 calls");
    assert!(post_count.unwrap() >= 1, "POST should have at least 1 call");
}

#[test]
fn test_mixed_labels_counter() {
    let snapshotter = setup_recorder();

    fn_with_mixed_labels("read");
    fn_with_mixed_labels("write");

    let read_counter = get_counter_with_labels(
        snapshotter,
        "test_mixed_labels_total",
        &[("service", "api"), ("operation", "read")],
    );

    let write_counter = get_counter_with_labels(
        snapshotter,
        "test_mixed_labels_total",
        &[("service", "api"), ("operation", "write")],
    );

    assert!(read_counter.is_some(), "Counter with operation=read should exist");
    assert!(write_counter.is_some(), "Counter with operation=write should exist");
}

#[test]
fn test_mixed_labels_histogram() {
    let snapshotter = setup_recorder();

    fn_with_mixed_labels("delete");

    // Verify histogram was recorded with correct labels
    let histogram = has_histogram_with_labels(
        snapshotter,
        "test_mixed_labels_seconds",
        &[("service", "api"), ("operation", "delete")],
    );

    assert!(histogram, "Histogram with mixed labels should exist");
}

#[test]
fn test_full_instrumentation_success() {
    let snapshotter = setup_recorder();

    let before_errors = get_counter_with_labels(
        snapshotter,
        "test_full_errors_total",
        &[("service", "db"), ("table", "users"), ("operation", "select")],
    ).unwrap_or(0);

    let result = full_instrumented("users", "select");
    assert!(result.is_ok());

    // Verify counter with all labels
    let counter = get_counter_with_labels(
        snapshotter,
        "test_full_total",
        &[("service", "db"), ("table", "users"), ("operation", "select")],
    );
    assert!(counter.is_some(), "Full counter should exist with all labels");

    // Verify error counter NOT incremented
    let after_errors = get_counter_with_labels(
        snapshotter,
        "test_full_errors_total",
        &[("service", "db"), ("table", "users"), ("operation", "select")],
    ).unwrap_or(0);
    assert_eq!(after_errors, before_errors, "Error counter should not increment on success");
}

#[test]
fn test_full_instrumentation_failure() {
    let snapshotter = setup_recorder();

    let before_errors = get_counter_with_labels(
        snapshotter,
        "test_full_errors_total",
        &[("service", "db"), ("table", "orders"), ("operation", "delete")],
    ).unwrap_or(0);

    let result = full_instrumented_fails("orders", "delete");
    assert!(result.is_err());

    // Verify error counter IS incremented
    let after_errors = get_counter_with_labels(
        snapshotter,
        "test_full_errors_total",
        &[("service", "db"), ("table", "orders"), ("operation", "delete")],
    ).unwrap_or(0);
    assert_eq!(
        after_errors - before_errors, 1,
        "Error counter SHOULD increment on failure"
    );
}
