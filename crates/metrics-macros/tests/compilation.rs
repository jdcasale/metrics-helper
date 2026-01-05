//! Integration tests for the instrument_metrics macro.
//!
//! These tests verify that the macro generates valid, compilable code
//! for various function signatures and configurations.

use metrics_helper_macros::instrument_metrics;

// ============================================================================
// Basic Usage Tests
// ============================================================================

/// Test: Counter only on a sync function
#[instrument_metrics(counter = "basic_sync_calls_total")]
fn basic_sync_counter() -> i32 {
    42
}

/// Test: Counter only on an async function
#[instrument_metrics(counter = "basic_async_calls_total")]
async fn basic_async_counter() -> i32 {
    42
}

/// Test: Histogram (timing) on a sync function
#[instrument_metrics(histogram = "sync_duration_seconds")]
fn sync_with_timing() -> String {
    "hello".to_string()
}

/// Test: Histogram (timing) on an async function
#[instrument_metrics(histogram = "async_duration_seconds")]
async fn async_with_timing() -> String {
    "hello".to_string()
}

// ============================================================================
// Result Return Type Tests (for error_counter)
// ============================================================================

/// Test: Error counter on function returning Result (Ok path)
#[instrument_metrics(counter = "result_calls_total", error_counter = "result_errors_total")]
fn sync_result_ok() -> Result<i32, &'static str> {
    Ok(42)
}

/// Test: Error counter on function returning Result (Err path)
#[instrument_metrics(counter = "result_calls_total", error_counter = "result_errors_total")]
fn sync_result_err() -> Result<i32, &'static str> {
    Err("something went wrong")
}

/// Test: Async function with error counter
#[instrument_metrics(
    counter = "async_result_calls_total",
    error_counter = "async_result_errors_total"
)]
async fn async_result() -> Result<String, std::io::Error> {
    Ok("success".to_string())
}

// ============================================================================
// Static Labels Tests
// ============================================================================

/// Test: Single static label
#[instrument_metrics(counter = "labeled_calls_total", labels(service = "test"))]
fn single_static_label() {}

/// Test: Multiple static labels
#[instrument_metrics(
    counter = "multi_labeled_calls_total",
    labels(service = "api", version = "v1", environment = "test")
)]
fn multiple_static_labels() {}

// ============================================================================
// Dynamic Labels Tests (from function parameters)
// ============================================================================

/// Test: Single dynamic label from function parameter
#[instrument_metrics(counter = "dynamic_calls_total", labels(method))]
fn single_dynamic_label(method: &str) -> &str {
    method
}

/// Test: Multiple dynamic labels from function parameters
#[instrument_metrics(counter = "multi_dynamic_calls_total", labels(operation, tenant_id))]
fn multiple_dynamic_labels(operation: &str, tenant_id: &str) {
    let _ = (operation, tenant_id);
}

/// Test: Dynamic label with numeric type (must implement Display)
#[instrument_metrics(counter = "numeric_label_calls_total", labels(user_id))]
fn dynamic_label_numeric(user_id: u64) -> u64 {
    user_id
}

// ============================================================================
// Mixed Labels Tests (static + dynamic)
// ============================================================================

/// Test: Mix of static and dynamic labels
#[instrument_metrics(
    counter = "mixed_labels_calls_total",
    histogram = "mixed_labels_duration_seconds",
    labels(service = "api", method, tenant_id)
)]
fn mixed_labels(method: &str, tenant_id: &str, _payload: &[u8]) {
    let _ = (method, tenant_id);
}

/// Test: Full configuration with mixed labels on async Result function
#[instrument_metrics(
    counter = "full_config_calls_total",
    histogram = "full_config_duration_seconds",
    error_counter = "full_config_errors_total",
    labels(service = "database", table = "users", operation)
)]
async fn full_configuration(operation: &str) -> Result<Vec<String>, &'static str> {
    let _ = operation;
    Ok(vec!["user1".to_string(), "user2".to_string()])
}

// ============================================================================
// Struct Field Labels Tests
// ============================================================================

/// Test struct for field-based labels
#[derive(Clone)]
struct Request {
    method: String,
    path: String,
    user_id: u64,
}

/// Test: Single struct field as label
#[instrument_metrics(counter = "struct_field_calls_total", labels(request.method))]
fn single_struct_field_label(request: &Request) {
    let _ = request;
}

/// Test: Multiple struct fields as labels
#[instrument_metrics(
    counter = "multi_struct_field_calls_total",
    labels(request.method, request.path)
)]
fn multiple_struct_field_labels(request: &Request) {
    let _ = request;
}

/// Test: Mix of struct field and static labels
#[instrument_metrics(
    counter = "mixed_struct_static_calls_total",
    labels(service = "api", request.method, request.path)
)]
fn mixed_struct_and_static_labels(request: &Request) {
    let _ = request;
}

/// Test: Mix of struct field and simple parameter labels
#[instrument_metrics(
    counter = "mixed_struct_param_calls_total",
    labels(request.method, operation)
)]
fn mixed_struct_and_param_labels(request: &Request, operation: &str) {
    let _ = (request, operation);
}

/// Test: Struct field with explicit key name
#[instrument_metrics(
    counter = "explicit_key_calls_total",
    labels(http_method = request.method)
)]
fn explicit_key_for_struct_field(request: &Request) {
    let _ = request;
}

/// Test: Full configuration with struct field labels
#[instrument_metrics(
    counter = "full_struct_calls_total",
    histogram = "full_struct_duration_seconds",
    error_counter = "full_struct_errors_total",
    labels(service = "api", request.method, request.path, request.user_id)
)]
fn full_struct_configuration(request: &Request) -> Result<(), &'static str> {
    let _ = request;
    Ok(())
}

/// Test: Nested struct field access
struct Nested {
    inner: Request,
}

#[instrument_metrics(counter = "nested_field_calls_total", labels(nested.inner.method))]
fn nested_struct_field(nested: &Nested) {
    let _ = nested;
}

// ============================================================================
// Edge Cases
// ============================================================================

/// Test: Function with no metrics attributes except labels (should still compile)
#[instrument_metrics(labels(tag = "value"))]
fn labels_only() {}

/// Test: Empty labels list
#[instrument_metrics(counter = "empty_labels_total", labels())]
fn empty_labels() {}

/// Test: Function with self parameter (method)
struct MyService;

impl MyService {
    #[instrument_metrics(counter = "service_method_calls_total", labels(action = "process"))]
    fn process(&self) -> bool {
        true
    }

    #[instrument_metrics(
        counter = "service_async_calls_total",
        histogram = "service_async_duration_seconds"
    )]
    async fn async_process(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

/// Test: Generic function
#[instrument_metrics(counter = "generic_calls_total")]
fn generic_function<T: std::fmt::Display>(value: T) -> String {
    value.to_string()
}

/// Test: Function with multiple generic parameters
#[instrument_metrics(
    counter = "multi_generic_calls_total",
    labels(type_name = "conversion")
)]
fn multi_generic<T, U>(input: T) -> U
where
    T: Into<U>,
{
    input.into()
}

// ============================================================================
// Runtime Tests
// ============================================================================

#[cfg(test)]
mod runtime_tests {
    use super::*;

    #[test]
    fn test_sync_functions_execute() {
        assert_eq!(basic_sync_counter(), 42);
        assert_eq!(sync_with_timing(), "hello");
        assert_eq!(sync_result_ok(), Ok(42));
        assert_eq!(sync_result_err(), Err("something went wrong"));
    }

    #[test]
    fn test_static_labels() {
        single_static_label();
        multiple_static_labels();
    }

    #[test]
    fn test_dynamic_labels() {
        assert_eq!(single_dynamic_label("GET"), "GET");
        multiple_dynamic_labels("insert", "tenant-123");
        assert_eq!(dynamic_label_numeric(42), 42);
    }

    #[test]
    fn test_mixed_labels() {
        mixed_labels("POST", "acme-corp", b"data");
    }

    #[test]
    fn test_method_on_struct() {
        let svc = MyService;
        assert!(svc.process());
    }

    #[test]
    fn test_generic_function() {
        assert_eq!(generic_function(42), "42");
        assert_eq!(generic_function("hello"), "hello");
    }

    #[test]
    fn test_edge_cases() {
        labels_only();
        empty_labels();
    }

    #[test]
    fn test_struct_field_labels() {
        let request = Request {
            method: "GET".to_string(),
            path: "/users".to_string(),
            user_id: 42,
        };

        single_struct_field_label(&request);
        multiple_struct_field_labels(&request);
        mixed_struct_and_static_labels(&request);
        mixed_struct_and_param_labels(&request, "read");
        explicit_key_for_struct_field(&request);
        assert!(full_struct_configuration(&request).is_ok());

        let nested = Nested { inner: request };
        nested_struct_field(&nested);
    }

    #[tokio::test]
    async fn test_async_functions() {
        assert_eq!(basic_async_counter().await, 42);
        assert_eq!(async_with_timing().await, "hello");
        assert!(async_result().await.is_ok());
    }

    #[tokio::test]
    async fn test_full_configuration() {
        let result = full_configuration("select").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_async_method() {
        let svc = MyService;
        assert!(svc.async_process().await.is_ok());
    }
}
