//! Proc-macros for idiomatic Prometheus metrics instrumentation
//!
//! Provides the `#[instrument_metrics]` attribute macro for ergonomic
//! function instrumentation with counters, histograms, and error tracking.
//!
//! # Quick Start
//!
//! ```ignore
//! use metrics_helper_macros::instrument_metrics;
//!
//! #[instrument_metrics(
//!     counter = "http_requests_total",
//!     histogram = "http_request_duration_seconds",
//!     error_counter = "http_request_errors_total",
//!     labels(endpoint = "/users", method),  // static + dynamic label
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
//! #[instrument_metrics(
//!     counter = "requests_total",
//!     labels(service = "api", version = "v1"),
//! )]
//! fn handle() { }
//! ```
//!
//! ## Dynamic Labels (from function parameters)
//!
//! Use just the parameter name (without a value) to capture it as a label.
//! The parameter must implement `Display`:
//!
//! ```ignore
//! #[instrument_metrics(
//!     counter = "db_queries_total",
//!     labels(
//!         table = "users",  // static: always "users"
//!         operation,        // dynamic: captured from function param
//!         tenant_id,        // dynamic: captured from function param
//!     ),
//! )]
//! async fn query(operation: &str, tenant_id: &str, limit: usize) -> Result<Data, Error> {
//!     // Metrics will include: table="users", operation=<value>, tenant_id=<value>
//! }
//! ```
//!
//! This generates metrics like:
//! ```text
//! db_queries_total{table="users", operation="select", tenant_id="acme-corp"} 1
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
use quote::quote;
use syn::{parse_macro_input, ItemFn, ReturnType};

/// Configuration for a single label
#[derive(Debug, Clone)]
struct Label {
    key: String,
    value: LabelValue,
}

#[derive(Debug, Clone)]
enum LabelValue {
    /// Static string value: `method = "sync"`
    Static(String),
    /// Dynamic value from function argument: `node_id` (uses Display)
    Dynamic(String),
}

/// Parsed attributes for the instrument_metrics macro
#[derive(Debug, FromMeta)]
#[darling(allow_unknown_fields)]
struct InstrumentMetricsArgs {
    /// Counter to increment on each call
    #[darling(default)]
    counter: Option<String>,

    /// Histogram to record duration
    #[darling(default)]
    histogram: Option<String>,

    /// Counter to increment on error (Result::Err)
    #[darling(default)]
    error_counter: Option<String>,
    // Note: labels are parsed directly from meta in parse_labels_from_meta()
    // rather than through darling, since darling can't handle the nested syntax
}

/// Attribute macro for instrumenting functions with metrics.
///
/// # Usage
///
/// ```ignore
/// #[instrument_metrics(
///     counter = "sync_requests_total",
///     histogram = "sync_request_duration_seconds",
///     error_counter = "sync_errors_total",
///     labels(method = "sync"),
/// )]
/// async fn sync(&self, request: Request<SyncRequest>) -> Result<Response<SyncResponse>, Status> {
///     // ...
/// }
/// ```
///
/// # Attributes
///
/// - `counter`: Name of counter to increment on each call
/// - `histogram`: Name of histogram to record call duration (seconds)
/// - `error_counter`: Name of counter to increment when function returns `Err`
///   (only works for functions returning `Result`)
/// - `labels(...)`: Labels to attach to all metrics (see below)
///
/// # Labels
///
/// Labels support two syntaxes:
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
/// #[instrument_metrics(
///     counter = "requests_total",
///     labels(method, user_id),  // captures function params
/// )]
/// fn handle(method: &str, user_id: u64, payload: Bytes) { }
/// ```
///
/// You can mix both styles:
/// ```ignore
/// labels(service = "api", method, tenant_id)
/// ```
///
/// # Feature Gating
///
/// All metric recording is wrapped in `#[cfg(feature = "metrics")]` so there's
/// zero overhead when compiled without the metrics feature.
#[proc_macro_attribute]
pub fn instrument_metrics(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr_args = match NestedMeta::parse_meta_list(attr.into()) {
        Ok(v) => v,
        Err(e) => return TokenStream::from(Error::from(e).write_errors()),
    };
    let input_fn = parse_macro_input!(item as ItemFn);

    match instrument_metrics_impl(attr_args, input_fn) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.write_errors().into(),
    }
}

fn instrument_metrics_impl(
    attr_args: Vec<NestedMeta>,
    input_fn: ItemFn,
) -> Result<TokenStream2, Error> {
    // Parse attributes using darling
    let args = InstrumentMetricsArgs::from_list(&attr_args)?;

    // Parse labels from the nested meta
    let labels = parse_labels_from_meta(&attr_args)?;

    // Extract function components
    let attrs = &input_fn.attrs;
    let vis = &input_fn.vis;
    let sig = &input_fn.sig;
    let block = &input_fn.block;
    let is_async = sig.asyncness.is_some();

    // Check if return type is Result for error tracking
    let returns_result = matches!(&sig.output, ReturnType::Type(_, ty) if is_result_type(ty));

    // Build label tokens for metrics macros
    let label_tokens = build_label_tokens(&labels);

    // Build the counter increment code
    let counter_code = args.counter.as_ref().map(|name| {
        quote! {
            #[cfg(feature = "metrics")]
            ::metrics::counter!(#name #label_tokens).increment(1);
        }
    });

    // Build the histogram recording code
    let histogram_code = args.histogram.as_ref().map(|name| {
        quote! {
            #[cfg(feature = "metrics")]
            ::metrics::histogram!(#name #label_tokens).record(__metrics_start.elapsed().as_secs_f64());
        }
    });

    // Build the error counter code (only if returns Result)
    let error_counter_code = if returns_result {
        args.error_counter.as_ref().map(|name| {
            quote! {
                #[cfg(feature = "metrics")]
                if __metrics_result.is_err() {
                    ::metrics::counter!(#name #label_tokens).increment(1);
                }
            }
        })
    } else {
        None
    };

    // Determine if we need timing
    let needs_timing = args.histogram.is_some();

    // Build the instrumented function body
    let instrumented_body = if is_async {
        build_async_body(
            block,
            counter_code,
            histogram_code,
            error_counter_code,
            needs_timing,
        )
    } else {
        build_sync_body(
            block,
            counter_code,
            histogram_code,
            error_counter_code,
            needs_timing,
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

fn parse_labels_from_meta(attr_args: &[NestedMeta]) -> Result<Vec<Label>, Error> {
    let mut labels = Vec::new();

    for meta in attr_args {
        if let NestedMeta::Meta(syn::Meta::List(list)) = meta {
            if list.path.is_ident("labels") {
                // Parse the labels(...) content
                let nested = NestedMeta::parse_meta_list(list.tokens.clone())?;
                for label_meta in nested {
                    match label_meta {
                        NestedMeta::Meta(syn::Meta::NameValue(nv)) => {
                            // Static label: key = "value"
                            let key = nv
                                .path
                                .get_ident()
                                .map(|i| i.to_string())
                                .ok_or_else(|| {
                                    Error::custom("expected simple identifier for label key")
                                })?;

                            let value = match &nv.value {
                                syn::Expr::Lit(expr_lit) => match &expr_lit.lit {
                                    syn::Lit::Str(s) => s.value(),
                                    _ => {
                                        return Err(Error::custom(
                                            "expected string literal for label value",
                                        ))
                                    }
                                },
                                _ => {
                                    return Err(Error::custom(
                                        "expected string literal for label value",
                                    ))
                                }
                            };

                            labels.push(Label {
                                key,
                                value: LabelValue::Static(value),
                            });
                        }
                        NestedMeta::Meta(syn::Meta::Path(path)) => {
                            // Dynamic label: just the identifier name
                            let key =
                                path.get_ident().map(|i| i.to_string()).ok_or_else(|| {
                                    Error::custom("expected simple identifier for dynamic label")
                                })?;

                            labels.push(Label {
                                key: key.clone(),
                                value: LabelValue::Dynamic(key),
                            });
                        }
                        _ => {
                            return Err(Error::custom(
                                "expected `key = \"value\"` or `key` for labels",
                            ))
                        }
                    }
                }
            }
        }
    }

    Ok(labels)
}

fn build_label_tokens(labels: &[Label]) -> TokenStream2 {
    if labels.is_empty() {
        return quote! {};
    }

    let label_pairs: Vec<TokenStream2> = labels
        .iter()
        .map(|label| {
            let key = &label.key;
            match &label.value {
                LabelValue::Static(value) => {
                    quote! { #key => #value }
                }
                LabelValue::Dynamic(var_name) => {
                    let var_ident = syn::Ident::new(var_name, proc_macro2::Span::call_site());
                    quote! { #key => #var_ident.to_string() }
                }
            }
        })
        .collect();

    quote! { , #(#label_pairs),* }
}

fn build_async_body(
    block: &syn::Block,
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
