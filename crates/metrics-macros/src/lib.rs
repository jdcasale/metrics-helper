//! Proc-macros for idiomatic Prometheus metrics instrumentation
//!
//! Provides the `#[instrument]` attribute macro for ergonomic
//! function instrumentation with counters, histograms, and error tracking.
//!
//! # Quick Start
//!
//! Metric names are automatically derived from the function name:
//!
//! ```ignore
//! use metrics_helper_macros::instrument;
//!
//! // Auto-generates metrics:
//! // - counter: "get_users_total"
//! // - histogram: "get_users_duration_seconds"
//! // - error_counter: "get_users_errors_total"
//! #[instrument]
//! async fn get_users(method: &str) -> Result<Vec<User>, ApiError> {
//!     // Your code here...
//! }
//! ```
//!
//! You can also override specific metric names:
//!
//! ```ignore
//! #[instrument(
//!     counter = "http_requests_total",  // Override counter name
//!     labels(endpoint = "/users", method),
//! )]
//! async fn get_users(method: &str) -> Result<Vec<User>, ApiError> {
//!     // Your code here...
//! }
//! ```
//!
//! # Labels
//!
//! Labels can be either **static** (fixed values) or **dynamic** (from function parameters):
//!
//! ## Static Labels
//!
//! Use `key = "value"` syntax for labels with fixed values:
//!
//! ```ignore
//! #[instrument(labels(service = "api", version = "v1"))]
//! fn handle() { }
//! ```
//!
//! ## Dynamic Labels (from function parameters)
//!
//! Use just the parameter name (without a value) to capture it as a label.
//! The parameter must implement `Display`:
//!
//! ```ignore
//! #[instrument(labels(
//!     table = "users",  // static: always "users"
//!     operation,        // dynamic: captured from function param
//!     tenant_id,        // dynamic: captured from function param
//! ))]
//! async fn query(operation: &str, tenant_id: &str, limit: usize) -> Result<Data, Error> {
//!     // Metrics will include: table="users", operation=<value>, tenant_id=<value>
//! }
//! ```
//!
//! This generates metrics like:
//! ```text
//! query_total{table="users", operation="select", tenant_id="acme-corp"} 1
//! ```
//!
//! ## Dynamic Labels (from struct fields)
//!
//! Use dot notation (`param.field`) to capture struct fields as labels.
//! The field must implement `Display`:
//!
//! ```ignore
//! struct Request {
//!     method: String,
//!     path: String,
//!     user_id: u64,
//! }
//!
//! #[instrument(labels(
//!     service = "api",   // static label
//!     request.method,    // captures request.method as "method" label
//!     request.path,      // captures request.path as "path" label
//! ))]
//! fn handle_request(request: &Request) {
//!     // Metrics will include: service="api", method=<value>, path=<value>
//! }
//! ```
//!
//! You can also specify an explicit key name:
//!
//! ```ignore
//! #[instrument(labels(http_method = request.method))]
//! fn handle(request: &Request) { }
//! ```
//!
//! Nested field access is also supported:
//!
//! ```ignore
//! #[instrument(labels(ctx.request.method))]
//! fn process(ctx: &Context) { }
//! ```
//!
//! # Feature Gating
//!
//! All metric recording is wrapped in `#[cfg(feature = "metrics")]`, providing
//! zero overhead when compiled without the metrics feature enabled.

use darling::ast::NestedMeta;
use darling::{Error, FromMeta};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, ItemFn, ReturnType, Token};

#[derive(Debug)]
enum LabelValue {
    /// Static string value: `method = "sync"`
    Static(String),
    /// Dynamic value from an expression: `request.method` or just `method`
    /// The expression will have `.to_string()` called on it
    Dynamic(syn::Expr),
}

/// A single label item parsed from the labels(...) block
/// Supports:
/// - `key = "value"` - static label
/// - `key = expr` - dynamic label with explicit key
/// - `method` - dynamic label from variable (key = variable name)
/// - `request.method` - dynamic label from field (key = field name)
struct LabelItem {
    key: String,
    value: LabelValue,
}

impl Parse for LabelItem {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Try to parse as `key = value` first
        if input.peek(syn::Ident) && input.peek2(Token![=]) {
            let key: syn::Ident = input.parse()?;
            let _: Token![=] = input.parse()?;
            let value: syn::Expr = input.parse()?;

            // Check if it's a string literal (static label)
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &value
            {
                return Ok(LabelItem {
                    key: key.to_string(),
                    value: LabelValue::Static(s.value()),
                });
            }

            // Otherwise it's a dynamic expression
            return Ok(LabelItem {
                key: key.to_string(),
                value: LabelValue::Dynamic(value),
            });
        }

        // Otherwise parse as an expression (path or field access)
        let expr: syn::Expr = input.parse()?;

        match &expr {
            // Simple path like `method`
            syn::Expr::Path(path) => {
                let key = path
                    .path
                    .get_ident()
                    .map(|i| i.to_string())
                    .ok_or_else(|| {
                        syn::Error::new_spanned(&expr, "expected simple identifier for label")
                    })?;
                Ok(LabelItem {
                    key,
                    value: LabelValue::Dynamic(expr),
                })
            }
            // Field access like `request.method`
            syn::Expr::Field(field) => {
                let key = field.member.to_token_stream().to_string();
                Ok(LabelItem {
                    key,
                    value: LabelValue::Dynamic(expr),
                })
            }
            _ => Err(syn::Error::new_spanned(
                &expr,
                "expected identifier, field access (e.g., request.method), or key = value",
            )),
        }
    }
}

impl LabelItem {
    /// Generate the variable name used to capture this label's value
    fn capture_var_name(&self) -> syn::Ident {
        syn::Ident::new(
            &format!("__metrics_label_{}", self.key),
            proc_macro2::Span::call_site(),
        )
    }

    /// Generate code to capture dynamic label values upfront.
    /// Returns None for static labels (no capture needed).
    fn capture_statement(&self) -> Option<proc_macro2::TokenStream> {
        match &self.value {
            LabelValue::Static(_) => None,
            LabelValue::Dynamic(expr) => {
                let var_name = self.capture_var_name();
                Some(quote! {
                    let #var_name = (#expr).to_string();
                })
            }
        }
    }

    /// Generate the label key-value pair for use in metrics macros.
    /// For dynamic labels, references the captured variable.
    fn to_token_stream(&self) -> proc_macro2::TokenStream {
        let key = &self.key;
        match &self.value {
            LabelValue::Static(value) => {
                quote! { #key => #value }
            }
            LabelValue::Dynamic(_) => {
                let var_name = self.capture_var_name();
                quote! { #key => #var_name.clone() }
            }
        }
    }
}

/// Parsed attributes for the instrument macro
#[derive(Debug, FromMeta)]
#[darling(allow_unknown_fields)]
struct InstrumentArgs {
    /// Counter name override (default: `{fn_name}_total`)
    #[darling(default)]
    counter: Option<String>,

    /// Histogram name override (default: `{fn_name}_duration_seconds`)
    #[darling(default)]
    histogram: Option<String>,

    /// Error counter name override (default: `{fn_name}_errors_total`)
    #[darling(default)]
    error_counter: Option<String>,
    // Note: labels are parsed directly from meta in parse_labels_from_meta()
    // rather than through darling, since darling can't handle the nested syntax
}

/// Attribute macro for instrumenting functions with metrics.
///
/// Metric names are automatically derived from the function name:
/// - Counter: `{fn_name}_total`
/// - Histogram: `{fn_name}_duration_seconds`
/// - Error counter: `{fn_name}_errors_total`
///
/// # Usage
///
/// ```ignore
/// // Simple usage - all metrics auto-derived from function name
/// #[instrument]
/// async fn sync_data() -> Result<(), Error> {
///     // Generates: sync_data_total, sync_data_duration_seconds, sync_data_errors_total
/// }
///
/// // With labels
/// #[instrument(labels(method = "sync"))]
/// async fn sync(&self, request: Request<SyncRequest>) -> Result<Response<SyncResponse>, Status> {
///     // ...
/// }
///
/// // Override specific metric names
/// #[instrument(counter = "custom_requests_total")]
/// fn handle_request() {
///     // Uses custom_requests_total but auto-derives histogram name
/// }
/// ```
///
/// # Attributes
///
/// - `counter`: Override counter name (default: `{fn_name}_total`)
/// - `histogram`: Override histogram name (default: `{fn_name}_duration_seconds`)
/// - `error_counter`: Override error counter name (default: `{fn_name}_errors_total`)
/// - `labels(...)`: Labels to attach to all metrics
///
/// # Labels
///
/// Labels support multiple syntaxes:
///
/// ## Static Labels
/// Use `key = "value"` for fixed label values:
/// ```ignore
/// labels(service = "api", version = "v1")
/// ```
///
/// ## Dynamic Labels (from function parameters)
/// Use just the parameter name to capture its value at runtime.
/// The parameter must implement `Display`:
/// ```ignore
/// #[instrument(labels(method, user_id))]
/// fn handle(method: &str, user_id: u64, payload: Bytes) { }
/// ```
///
/// ## Dynamic Labels (from struct fields)
/// Use dot notation to capture struct field values:
/// ```ignore
/// #[instrument(labels(request.method, request.path))]
/// fn handle(request: &Request) { }
/// ```
///
/// You can also use an explicit key: `labels(http_method = request.method)`
///
/// You can mix all styles:
/// ```ignore
/// labels(service = "api", method, request.path)
/// ```
///
/// # Feature Gating
///
/// All metric recording is wrapped in `#[cfg(feature = "metrics")]` so there's
/// zero overhead when compiled without the metrics feature.
#[proc_macro_attribute]
pub fn instrument(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr_args = match NestedMeta::parse_meta_list(attr.into()) {
        Ok(v) => v,
        Err(e) => return TokenStream::from(Error::from(e).write_errors()),
    };
    let input_fn = parse_macro_input!(item as ItemFn);

    match instrument_impl(attr_args, input_fn) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.write_errors().into(),
    }
}

fn instrument_impl(attr_args: Vec<NestedMeta>, input_fn: ItemFn) -> Result<TokenStream2, Error> {
    // Parse attributes using darling
    let args = InstrumentArgs::from_list(&attr_args)?;

    // Parse labels from the nested meta
    let labels = parse_labels_from_meta(&attr_args)?;

    // Extract function components
    let attrs = &input_fn.attrs;
    let vis = &input_fn.vis;
    let sig = &input_fn.sig;
    let block = &input_fn.block;
    let is_async = sig.asyncness.is_some();
    let fn_name = sig.ident.to_string();

    // Check if return type is Result for error tracking
    let returns_result = matches!(&sig.output, ReturnType::Type(_, ty) if is_result_type(ty));

    // Derive metric names from function name (with optional overrides)
    let counter_name = args
        .counter
        .unwrap_or_else(|| format!("{}_total", fn_name));
    let histogram_name = args
        .histogram
        .unwrap_or_else(|| format!("{}_duration_seconds", fn_name));
    let error_counter_name = args
        .error_counter
        .unwrap_or_else(|| format!("{}_errors_total", fn_name));

    // Build label captures (evaluated upfront before async block or function body)
    // This ensures we capture values before they might be moved/consumed
    let label_captures = build_label_captures(&labels);

    // Build label tokens for metrics macros (references the captured values)
    let label_tokens = build_label_tokens(&labels);

    // Build the counter increment code
    let counter_code = quote! {
        #[cfg(feature = "metrics")]
        ::metrics::counter!(#counter_name #label_tokens).increment(1);
    };

    // Build the histogram recording code
    let histogram_code = quote! {
        #[cfg(feature = "metrics")]
        ::metrics::histogram!(#histogram_name #label_tokens).record(__metrics_start.elapsed().as_secs_f64());
    };

    // Build the error counter code (only if returns Result)
    let error_counter_code = if returns_result {
        Some(quote! {
            #[cfg(feature = "metrics")]
            if __metrics_result.is_err() {
                ::metrics::counter!(#error_counter_name #label_tokens).increment(1);
            }
        })
    } else {
        None
    };

    // Build the instrumented function body
    let instrumented_body = if is_async {
        build_async_body(
            block,
            label_captures,
            Some(counter_code),
            Some(histogram_code),
            error_counter_code,
            true, // always need timing now
        )
    } else {
        build_sync_body(
            block,
            label_captures,
            Some(counter_code),
            Some(histogram_code),
            error_counter_code,
            true, // always need timing now
        )
    };

    // Reconstruct the function
    Ok(quote! {
        #(#attrs)*
        #vis #sig {
            #instrumented_body
        }
    })
}

fn parse_labels_from_meta(attr_args: &[NestedMeta]) -> Result<Vec<LabelItem>, Error> {
    for meta in attr_args {
        if let NestedMeta::Meta(syn::Meta::List(list)) = meta {
            if list.path.is_ident("labels") {
                // Parse the labels(...) content as comma-separated LabelItems
                let parser = Punctuated::<LabelItem, Token![,]>::parse_terminated;
                let items = parser
                    .parse2(list.tokens.clone())
                    .map_err(|e: syn::Error| Error::custom(e.to_string()))?;
                return Ok(items.into_iter().collect());
            }
        }
    }

    Ok(Vec::new())
}

/// Generate code to capture all dynamic label values upfront.
/// This must be called before any code that might move/consume the labeled values.
fn build_label_captures(labels: &[LabelItem]) -> TokenStream2 {
    let captures: Vec<TokenStream2> = labels
        .iter()
        .filter_map(|label| label.capture_statement())
        .collect();

    if captures.is_empty() {
        quote! {}
    } else {
        quote! { #(#captures)* }
    }
}

fn build_label_tokens(labels: &[LabelItem]) -> TokenStream2 {
    if labels.is_empty() {
        return quote! {};
    }

    let label_pairs: Vec<TokenStream2> =
        labels.iter().map(|label| label.to_token_stream()).collect();

    quote! { , #(#label_pairs),* }
}

fn build_async_body(
    block: &syn::Block,
    label_captures: TokenStream2,
    counter_code: Option<TokenStream2>,
    histogram_code: Option<TokenStream2>,
    error_counter_code: Option<TokenStream2>,
    needs_timing: bool,
) -> TokenStream2 {
    let timing_start = if needs_timing {
        quote! { let __metrics_start = ::std::time::Instant::now(); }
    } else {
        quote! {}
    };

    let counter = counter_code.unwrap_or_else(|| quote! {});
    let histogram = histogram_code.unwrap_or_else(|| quote! {});
    let error_counter = error_counter_code.unwrap_or_else(|| quote! {});

    quote! {
        // Capture dynamic label values upfront before async block
        #label_captures

        #counter
        #timing_start

        let __metrics_result = async #block.await;

        #histogram
        #error_counter

        __metrics_result
    }
}

fn build_sync_body(
    block: &syn::Block,
    label_captures: TokenStream2,
    counter_code: Option<TokenStream2>,
    histogram_code: Option<TokenStream2>,
    error_counter_code: Option<TokenStream2>,
    needs_timing: bool,
) -> TokenStream2 {
    let timing_start = if needs_timing {
        quote! { let __metrics_start = ::std::time::Instant::now(); }
    } else {
        quote! {}
    };

    let counter = counter_code.unwrap_or_else(|| quote! {});
    let histogram = histogram_code.unwrap_or_else(|| quote! {});
    let error_counter = error_counter_code.unwrap_or_else(|| quote! {});

    quote! {
        // Capture dynamic label values upfront
        #label_captures

        #counter
        #timing_start

        let __metrics_result = #block;

        #histogram
        #error_counter

        __metrics_result
    }
}

fn is_result_type(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Result";
        }
    }
    false
}

#[cfg(test)]
mod tests {
    // Compile tests are better done with trybuild in integration tests
}
