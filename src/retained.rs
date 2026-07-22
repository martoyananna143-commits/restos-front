//! Keep last successful API payload while `use_resource` is in the loading (`None`) state,
//! and avoid redundant signal writes when the new value equals the previous one.

use dioxus::prelude::*;

/// Copies `Some(Ok(value))` from `read()` into `cache` only when it differs (`PartialEq`).
pub fn cache_ok_resource<T: Clone + PartialEq + 'static>(
    read: impl Fn() -> Option<Result<T, String>> + Copy + 'static,
    mut cache: Signal<Option<T>>,
) {
    use_effect(move || {
        if let Some(Ok(v)) = read() {
            if cache.read().as_ref() != Some(&v) {
                cache.set(Some(v));
            }
        }
    });
}

/// Prefer live `Ok` from the resource; on transient reload (`None`) fall back to `stale`;
/// on `Err`, keep `stale` if any (otherwise surface the error).
pub fn merge_ok_or_stale<T: Clone>(
    live: Option<Result<T, String>>,
    stale: Option<T>,
) -> Result<Option<T>, String> {
    match live {
        Some(Ok(v)) => Ok(Some(v)),
        Some(Err(e)) => {
            if let Some(v) = stale {
                Ok(Some(v))
            } else {
                Err(e)
            }
        }
        None => Ok(stale),
    }
}
