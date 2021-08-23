//! Macros for use with Madsim

use proc_macro::TokenStream;
use quote::quote;

#[allow(clippy::needless_doctest_main)]
/// Marks async function to be executed by the selected runtime. This macro
/// helps set up a `Runtime` without requiring the user to use
/// [Runtime](../madsim/struct.Runtime.html) directly.
///
/// # Example
///
/// ```
/// #[madsim::main]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[madsim::main]`
///
/// ```
/// fn main() {
///     madsim::Runtime::new().block_on(async {
///         println!("Hello world");
///     });
/// }
/// ```
#[proc_macro_attribute]
pub fn main(args: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemFn);
    let args = syn::parse_macro_input!(args as syn::AttributeArgs);

    parse(input, args, false).unwrap_or_else(|e| e.to_compile_error().into())
}

/// Marks async function to be executed by runtime, suitable to test environment.
///
/// # Example
/// ```no_run
/// #[madsim::test]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
#[proc_macro_attribute]
pub fn test(args: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemFn);
    let args = syn::parse_macro_input!(args as syn::AttributeArgs);

    parse(input, args, true).unwrap_or_else(|e| e.to_compile_error().into())
}

fn parse(
    mut input: syn::ItemFn,
    _args: syn::AttributeArgs,
    is_test: bool,
) -> Result<TokenStream, syn::Error> {
    if input.sig.asyncness.take().is_none() {
        let msg = "the `async` keyword is missing from the function declaration";
        return Err(syn::Error::new_spanned(input.sig.fn_token, msg));
    }

    let header = if is_test {
        quote! {
            #[::core::prelude::v1::test]
        }
    } else {
        quote! {}
    };

    let body = &input.block;
    let brace_token = input.block.brace_token;
    input.block = syn::parse2(quote! {
        {
            use std::time::{Duration, SystemTime};
            let seed: u64 = if let Ok(seed_str) = std::env::var("MADSIM_TEST_SEED") {
                seed_str.parse().expect("MADSIM_TEST_SEED should be an integer")
            } else {
                SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
            };
            let mut count: u64 = if let Ok(num_str) = std::env::var("MADSIM_TEST_NUM") {
                num_str.parse().expect("MADSIM_TEST_NUM should be an integer")
            } else {
                1
            };
            let time_limit_s = std::env::var("MADSIM_TEST_TIME_LIMIT").ok().map(|num_str| {
                num_str.parse::<f64>().expect("MADSIM_TEST_TIME_LIMIT should be an number")
            });
            let check = std::env::var("MADSIM_TEST_CHECK_DETERMINISTIC").is_ok();
            if check {
                count = 2;
            }
            let mut rand_log = None;
            for i in 0..count {
                let seed = if check { seed } else { seed + i };
                let rand_log0 = rand_log.take();
                let ret = std::panic::catch_unwind(move || {
                    let mut rt = madsim::Runtime::new_with_seed(seed);
                    if check {
                        rt.enable_deterministic_check(rand_log0);
                    }
                    if let Some(limit) = time_limit_s {
                        rt.set_time_limit(Duration::from_secs_f64(limit));
                    }
                    rt.block_on(async #body);
                    rt.take_rand_log()
                });
                if let Err(e) = ret {
                    println!("MADSIM_TEST_SEED={}", seed);
                    std::panic::resume_unwind(e);
                }
                rand_log = ret.unwrap();
            }
        }
    })
    .expect("Parsing failure");
    input.block.brace_token = brace_token;

    let result = quote! {
        #header
        #input
    };

    Ok(result.into())
}