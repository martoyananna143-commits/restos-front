//! Telegram Web App API bindings
//! Minimal bindings for UX features (expand, close, theme)

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    // Core Web App methods (internal - use safe wrappers below)
    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp"], js_name = ready)]
    fn tg_ready();

    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp"], js_name = close)]
    fn tg_close();

    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp"], js_name = expand)]
    fn tg_expand();

    // WHY `catch`: these methods throw WebAppMethodUnsupported on older TG versions
    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp"], js_name = requestFullscreen, catch)]
    fn tg_request_fullscreen() -> Result<(), JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp"], js_name = exitFullscreen, catch)]
    fn tg_exit_fullscreen() -> Result<(), JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp"], js_name = lockOrientation, catch)]
    fn tg_lock_orientation() -> Result<(), JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp"], js_name = unlockOrientation, catch)]
    fn tg_unlock_orientation() -> Result<(), JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp"], js_name = isFullscreen, getter)]
    fn tg_is_fullscreen() -> bool;

    // Theme
    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp"], js_name = colorScheme, getter)]
    pub fn color_scheme() -> String;

    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp"], js_name = themeParams, getter)]
    fn theme_params_raw() -> JsValue;

    // MainButton
    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp", "MainButton"], js_name = setText)]
    pub fn main_button_set_text(text: &str);

    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp", "MainButton"])]
    pub fn show() -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp", "MainButton"])]
    pub fn hide() -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp", "MainButton"], js_name = showProgress)]
    pub fn main_button_show_progress(leave_active: bool);

    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp", "MainButton"], js_name = hideProgress)]
    pub fn main_button_hide_progress();

    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp", "MainButton"])]
    pub fn enable();

    #[wasm_bindgen(js_namespace = ["window", "Telegram", "WebApp", "MainButton"])]
    pub fn disable();
}

/// Check if running inside Telegram Web App
#[inline]
pub fn is_telegram_webapp() -> bool {
    web_sys::window()
        .and_then(|w| js_sys::Reflect::get(&w, &"Telegram".into()).ok())
        .map(|t| !t.is_undefined())
        .unwrap_or(false)
}

// ==================== Safe wrappers ====================

/// Safe wrapper for ready() - only calls if in Telegram
#[inline]
pub fn ready() {
    if is_telegram_webapp() {
        tg_ready();
    }
}

/// Safe wrapper for close() - only calls if in Telegram
#[inline]
pub fn close() {
    if is_telegram_webapp() {
        tg_close();
    }
}

/// Safe wrapper for expand() - only calls if in Telegram
#[inline]
pub fn expand() {
    if is_telegram_webapp() {
        tg_expand();
    }
}

/// Safe wrapper for requestFullscreen() — silently ignored on unsupported versions
#[inline]
pub fn request_fullscreen() {
    if is_telegram_webapp() {
        let _ = tg_request_fullscreen();
    }
}

/// Safe wrapper for exitFullscreen() — silently ignored on unsupported versions
#[inline]
pub fn exit_fullscreen() {
    if is_telegram_webapp() {
        let _ = tg_exit_fullscreen();
    }
}

/// Safe wrapper for lockOrientation() — silently ignored on unsupported versions
#[inline]
pub fn lock_orientation() {
    if is_telegram_webapp() {
        let _ = tg_lock_orientation();
    }
}

/// Safe wrapper for unlockOrientation() — silently ignored on unsupported versions
#[inline]
pub fn unlock_orientation() {
    if is_telegram_webapp() {
        let _ = tg_unlock_orientation();
    }
}

/// Check if app is in fullscreen mode
#[inline]
pub fn is_fullscreen() -> bool {
    if is_telegram_webapp() {
        tg_is_fullscreen()
    } else {
        false
    }
}

/// Theme colors for styling
pub struct ThemeColors {
    pub bg_color: String,
    pub text_color: String,
    pub hint_color: String,
    pub button_color: String,
    pub button_text_color: String,
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            bg_color: "#ffffff".into(),
            text_color: "#000000".into(),
            hint_color: "#999999".into(),
            button_color: "#3390ec".into(),
            button_text_color: "#ffffff".into(),
        }
    }
}

/// Get theme colors from Telegram
pub fn get_theme_colors() -> ThemeColors {
    let params = theme_params_raw();
    if params.is_undefined() || params.is_null() {
        return ThemeColors::default();
    }

    let get_color = |key: &str, default: &str| -> String {
        js_sys::Reflect::get(&params, &key.into())
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_else(|| default.into())
    };

    ThemeColors {
        bg_color: get_color("bg_color", "#ffffff"),
        text_color: get_color("text_color", "#000000"),
        hint_color: get_color("hint_color", "#999999"),
        button_color: get_color("button_color", "#3390ec"),
        button_text_color: get_color("button_text_color", "#ffffff"),
    }
}

/// Setup MainButton click handler
pub fn on_main_button_click(callback: impl Fn() + 'static) {
    let closure = Closure::wrap(Box::new(callback) as Box<dyn Fn()>);
    
    if let Some(window) = web_sys::window() {
        if let Ok(telegram) = js_sys::Reflect::get(&window, &"Telegram".into()) {
            if let Ok(webapp) = js_sys::Reflect::get(&telegram, &"WebApp".into()) {
                if let Ok(main_button) = js_sys::Reflect::get(&webapp, &"MainButton".into()) {
                    let _ = js_sys::Reflect::set(
                        &main_button,
                        &"onClick".into(),
                        closure.as_ref().unchecked_ref(),
                    );
                }
            }
        }
    }
    
    closure.forget();
}
