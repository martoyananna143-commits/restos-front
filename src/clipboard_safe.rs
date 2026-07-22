//! Clipboard via JS: Promise always resolves (empty string = OK), so wasm-bindgen never sees an uncaught throw from clipboard imports.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(inline_js = r#"
export function restos_copy_text_safe(text) {
  return new Promise(function (resolve) {
    (async function () {
      try {
        if (typeof navigator !== "undefined" && navigator.clipboard &&
            typeof window !== "undefined" && window.isSecureContext) {
          await navigator.clipboard.writeText(text);
          resolve("");
          return;
        }
      } catch (e) {
        /* fall through to fallback */
      }
      try {
        var root = document.body || document.documentElement;
        if (!root) {
          resolve("Нет document.body для копирования");
          return;
        }
        var ta = document.createElement("textarea");
        ta.value = text;
        ta.setAttribute("readonly", "");
        ta.style.position = "fixed";
        ta.style.left = "-9999px";
        ta.style.top = "0";
        ta.style.opacity = "0";
        root.appendChild(ta);
        ta.focus();
        ta.select();
        try {
          ta.setSelectionRange(0, ta.value.length);
        } catch (e3) {
          /* ignore */
        }
        var ok = document.execCommand("copy");
        root.removeChild(ta);
        if (!ok) {
          resolve("execCommand('copy') вернул false");
          return;
        }
        resolve("");
      } catch (e2) {
        var m = (e2 && e2.message) ? String(e2.message) : String(e2);
        resolve(m || "ошибка копирования");
      }
    })();
  });
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = restos_copy_text_safe)]
    fn restos_copy_text_safe_js(text: &str) -> js_sys::Promise;
}

pub async fn copy_text_async(text: &str) -> Result<(), String> {
    let p = restos_copy_text_safe_js(text);
    let v = JsFuture::from(p).await.map_err(js_err_string)?;
    if let Some(s) = v.as_string() {
        if s.is_empty() {
            Ok(())
        } else {
            Err(s)
        }
    } else {
        Err("неожиданный ответ при копировании".to_string())
    }
}

fn js_err_string(e: JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}
