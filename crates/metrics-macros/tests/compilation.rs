//! Integration tests for the instrument macro.
//!
//! These tests verify that the macro generates valid, compilable code
//! for various function signatures and configurations.

use metrics_helper_macros::instrument;

// ============================================================================
// Basic Usage Tests
// ============================================================================

/// Test: Simple sync function with auto-derived metric names
#[instrument]
fn basic_sync_counter() -> i32 {
    42
}

/// Test: Simple async function with auto-derived metric names
#[instrument]
async fn basic_async_counter() -> i32 {
    42
}

/// Test: Sync function with auto-derived metrics
#[instrument]
fn sync_with_timing() -> String {
    "hello".to_string()
}

/// Test: Async function with auto-derived metrics
#[instrument]
async fn async_with_timing() -> String {
    "hello".to_string()
}

// ============================================================================
// Result Return Type Tests (for error_counter)
// ============================================================================

/// Test: Error counter on function returning Result (Ok path)
#[instrument]
fn sync_result_ok() -> Result<i32, &'static str> {
    Ok(42)
}

/// Test: Error counter on function returning Result (Err path)
#[instrument]
fn sync_result_err() -> Result<i32, &'static str> {
    Err("something went wrong")
}

/// Test: Async function with error counter
#[instrument]
async fn async_result() -> Result<String, std::io::Error> {
    Ok("success".to_string())
}

// ============================================================================
// Static Labels Tests
// ============================================================================

/// Test: Single static label
#[instrument(labels(service = "test"))]
fn single_static_label() {}

/// Test: Multiple static labels
#[instrument(labels(service = "api", version = "v1", environment = "test"))]
fn multiple_static_labels() {}

// ============================================================================
// Dynamic Labels Tests (from function parameters)
// ============================================================================

/// Test: Single dynamic label from function parameter
#[instrument(labels(method))]
fn single_dynamic_label(method: &str) -> &str {
    method
}

/// Test: Multiple dynamic labels from function parameters
#[instrument(labels(operation, tenant_id))]
fn multiple_dynamic_labels(operation: &str, tenant_id: &str) {
    let _ = (operation, tenant_id);
}

/// Test: Dynamic label with numeric type (must implement Display)
#[instrument(labels(user_id))]
fn dynamic_label_numeric(user_id: u64) -> u64 {
    user_id
}

// ============================================================================
// Mixed Labels Tests (static + dynamic)
// ============================================================================

/// Test: Mix of static and dynamic labels
#[instrument(labels(service = "api", method, tenant_id))]
fn mixed_labels(method: &str, tenant_id: &str, _payload: &[u8]) {
    let _ = (method, tenant_id);
}

/// Test: Full configuration with mixed labels on async Result function
#[instrument(labels(service = "database", table = "users", operation))]
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
#[instrument(labels(request.method))]
fn single_struct_field_label(request: &Request) {
    let _ = request;
}

/// Test: Multiple struct fields as labels
#[instrument(labels(request.method, request.path))]
fn multiple_struct_field_labels(request: &Request) {
    let _ = request;
}

/// Test: Mix of struct field and static labels
#[instrument(labels(service = "api", request.method, request.path))]
fn mixed_struct_and_static_labels(request: &Request) {
    let _ = request;
}

/// Test: Mix of struct field and simple parameter labels
#[instrument(labels(request.method, operation))]
fn mixed_struct_and_param_labels(request: &Request, operation: &str) {
    let _ = (request, operation);
}

/// Test: Struct field with explicit key name
#[instrument(labels(http_method = request.method))]
fn explicit_key_for_struct_field(request: &Request) {
    let _ = request;
}

/// Test: Full configuration with struct field labels
#[instrument(labels(service = "api", request.method, request.path, request.user_id))]
fn full_struct_configuration(request: &Request) -> Result<(), &'static str> {
    let _ = request;
    Ok(())
}

/// Test: Nested struct field access
struct Nested {
    inner: Request,
}

#[instrument(labels(nested.inner.method))]
fn nested_struct_field(nested: &Nested) {
    let _ = nested;
}

/// Test: Async function that consumes the struct (verifies borrow fix)
/// Labels are captured upfront before the async block, so this compiles.
#[instrument(labels(request.method, request.path))]
async fn async_consumes_struct(request: Request) -> String {
    // Simulate consuming the request by moving it
    let owned = request;
    format!("processed {} {}", owned.method, owned.path)
}

/// Test: Sync function that moves the struct (verifies borrow fix)
#[instrument(labels(request.method))]
fn sync_consumes_struct(request: Request) -> String {
    let owned = request;
    owned.method
}

// ============================================================================
// Edge Cases
// ============================================================================

/// Test: Function with only labels (should still compile)
#[instrument(labels(tag = "value"))]
fn labels_only() {}

/// Test: Empty labels list
#[instrument(labels())]
fn empty_labels() {}

/// Test: Function with self parameter (method)
struct MyService;

impl MyService {
    #[instrument(labels(action = "process"))]
    fn process(&self) -> bool {
        true
    }

    #[instrument]
    async fn async_process(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

/// Test: Generic function
#[instrument]
fn generic_function<T: std::fmt::Display>(value: T) -> String {
    value.to_string()
}

/// Test: Function with multiple generic parameters
#[instrument(labels(type_name = "conversion"))]
fn multi_generic<T, U>(input: T) -> U
where
    T: Into<U>,
{
    input.into()
}

/// Test: Override specific metric names
#[instrument(counter = "custom_counter_name")]
fn with_custom_counter() -> i32 {
    100
}

/// Test: Override all metric names
#[instrument(
    counter = "custom_total",
    histogram = "custom_duration",
    error_counter = "custom_errors"
)]
fn with_all_custom_names() -> Result<(), &'static str> {
    Ok(())
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

    #[test]
    fn test_struct_consumed_sync() {
        // Test that sync functions can consume structs when using struct field labels
        let request = Request {
            method: "DELETE".to_string(),
            path: "/resource".to_string(),
            user_id: 1,
        };
        let result = sync_consumes_struct(request);
        assert_eq!(result, "DELETE");
    }

    #[tokio::test]
    async fn test_struct_consumed_async() {
        // Test that async functions can consume structs when using struct field labels
        // This verifies the borrow fix - labels are captured upfront
        let request = Request {
            method: "POST".to_string(),
            path: "/api/data".to_string(),
            user_id: 99,
        };
        let result = async_consumes_struct(request).await;
        assert_eq!(result, "processed POST /api/data");
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
