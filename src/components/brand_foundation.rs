//! Reusable RestOS brand primitives and the debug-only Stage 1 visual proof.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::{JsCast, JsValue};

const MOBILE_LAUNCH_MAX_WIDTH_PX: f64 = 600.0;
const MOBILE_LAUNCH_TIMEOUT_MS: u32 = 2_600;
const MOBILE_LAUNCH_SESSION_KEY: &str = "restos-launch-seen-v1";

fn mobile_launch_session_storage() -> Option<JsValue> {
    let window = web_sys::window()?;
    js_sys::Reflect::get(&window, &JsValue::from_str("sessionStorage")).ok()
}

fn mobile_launch_was_seen() -> bool {
    let Some(storage) = mobile_launch_session_storage() else {
        return false;
    };
    let Ok(get_item) = js_sys::Reflect::get(&storage, &JsValue::from_str("getItem")) else {
        return false;
    };
    let Ok(get_item) = get_item.dyn_into::<js_sys::Function>() else {
        return false;
    };
    get_item
        .call1(&storage, &JsValue::from_str(MOBILE_LAUNCH_SESSION_KEY))
        .ok()
        .is_some_and(|value| value.as_string().as_deref() == Some("1"))
}

fn mark_mobile_launch_seen() {
    let Some(storage) = mobile_launch_session_storage() else {
        return;
    };
    let Ok(set_item) = js_sys::Reflect::get(&storage, &JsValue::from_str("setItem")) else {
        return;
    };
    let Ok(set_item) = set_item.dyn_into::<js_sys::Function>() else {
        return;
    };
    let _ = set_item.call2(
        &storage,
        &JsValue::from_str(MOBILE_LAUNCH_SESSION_KEY),
        &JsValue::from_str("1"),
    );
}

fn reduced_motion_requested() -> bool {
    web_sys::window()
        .and_then(|window| {
            window
                .match_media("(prefers-reduced-motion: reduce)")
                .ok()
                .flatten()
        })
        .is_some_and(|query| query.matches())
}

fn mobile_launch_viewport_width() -> f64 {
    web_sys::window()
        .and_then(|window| window.inner_width().ok())
        .and_then(|width| width.as_f64())
        .unwrap_or(f64::NAN)
}

fn request_mobile_launch_playback() -> bool {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return false;
    };
    let Ok(Some(video)) = document.query_selector("#restos-mobile-launch-video") else {
        return false;
    };
    let _ = js_sys::Reflect::set(&video, &JsValue::from_str("muted"), &JsValue::TRUE);
    let Ok(play) = js_sys::Reflect::get(&video, &JsValue::from_str("play")) else {
        return false;
    };
    let Ok(play) = play.dyn_into::<js_sys::Function>() else {
        return false;
    };
    play.call0(&video).is_ok()
}

fn mobile_launch_allowed(width_px: f64, reduced_motion: bool) -> bool {
    width_px.is_finite()
        && width_px > 0.0
        && width_px <= MOBILE_LAUNCH_MAX_WIDTH_PX
        && !reduced_motion
}

#[component]
pub fn MobileLaunchAnimation() -> Element {
    let mut visible = use_signal(|| {
        !mobile_launch_was_seen()
            && mobile_launch_allowed(mobile_launch_viewport_width(), reduced_motion_requested())
    });
    let mut playback_failed = use_signal(|| false);

    use_effect(move || {
        if !visible() {
            return;
        }
        mark_mobile_launch_seen();
        if !request_mobile_launch_playback() {
            playback_failed.set(true);
        }
        spawn(async move {
            TimeoutFuture::new(MOBILE_LAUNCH_TIMEOUT_MS).await;
            visible.set(false);
        });
    });

    if !visible() {
        return rsx! {};
    }

    rsx! {
        section {
            class: "mobile-launch-animation",
            aria_label: "Запуск RestOS",
            if playback_failed() {
                img {
                    class: "mobile-launch-animation__fallback",
                    src: "/brand/r-icon-master.png",
                    alt: "",
                    aria_hidden: "true",
                }
            } else {
                video {
                    id: "restos-mobile-launch-video",
                    class: "mobile-launch-animation__video",
                    src: "/brand/restos-launch-mobile.mp4",
                    poster: "/icons/icon-512.png",
                    autoplay: true,
                    muted: true,
                    playsinline: true,
                    preload: "auto",
                    aria_hidden: "true",
                    oncanplay: move |_| {
                        if !request_mobile_launch_playback() {
                            playback_failed.set(true);
                        }
                    },
                    onended: move |_| visible.set(false),
                    onerror: move |_| playback_failed.set(true),
                }
            }
            button {
                class: "mobile-launch-animation__skip",
                r#type: "button",
                onclick: move |_| visible.set(false),
                "Пропустить"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BrandGlassVariant {
    Primary,
    Secondary,
    Selected,
    Attention,
}

impl BrandGlassVariant {
    fn class_name(self) -> &'static str {
        match self {
            Self::Primary => "brand-glass",
            Self::Secondary => "brand-glass brand-glass--secondary",
            Self::Selected => "brand-glass brand-glass--selected",
            Self::Attention => "brand-glass brand-glass--attention",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BrandButtonVariant {
    Primary,
    Secondary,
    Destructive,
}

impl BrandButtonVariant {
    fn class_name(self) -> &'static str {
        match self {
            Self::Primary => "brand-button brand-button--primary",
            Self::Secondary => "brand-button brand-button--secondary",
            Self::Destructive => "brand-button brand-button--destructive",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BrandStatusVariant {
    Excellent,
    Normal,
    Attention,
    Critical,
}

impl BrandStatusVariant {
    fn class_name(self) -> &'static str {
        match self {
            Self::Excellent => "brand-status brand-status--excellent",
            Self::Normal => "brand-status brand-status--normal",
            Self::Attention => "brand-status brand-status--attention",
            Self::Critical => "brand-status brand-status--critical",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Excellent => "✓",
            Self::Normal => "–",
            Self::Attention => "!",
            Self::Critical => "×",
        }
    }

    fn accessible_name(self) -> &'static str {
        match self {
            Self::Excellent => "Успешное состояние",
            Self::Normal => "Нормальное состояние",
            Self::Attention => "Требует внимания",
            Self::Critical => "Критическое состояние",
        }
    }
}

#[component]
pub fn BrandGlass(
    variant: BrandGlassVariant,
    #[props(default)] class: String,
    children: Element,
) -> Element {
    let classes = format!("{} {}", variant.class_name(), class);
    rsx! { section { class: classes, {children} } }
}

#[component]
pub fn BrandButton(
    variant: BrandButtonVariant,
    #[props(default)] disabled: bool,
    children: Element,
) -> Element {
    rsx! {
        button {
            class: variant.class_name(),
            r#type: "button",
            disabled,
            {children}
        }
    }
}

#[component]
pub fn BrandStatus(variant: BrandStatusVariant, label: String) -> Element {
    let accessible_label = format!("{}. {label}", variant.accessible_name());
    rsx! {
        span {
            class: variant.class_name(),
            aria_label: accessible_label,
            span { class: "brand-status__icon", aria_hidden: "true", "{variant.icon()}" }
            span { "{label}" }
        }
    }
}

#[cfg(debug_assertions)]
#[component]
pub fn BrandFoundationProof() -> Element {
    rsx! {
        main { class: "brand-foundation-proof", id: "brand-foundation-proof",
            header { class: "brand-proof-header",
                p { class: "brand-type-secondary", "INTERNAL / STAGE 1" }
                h1 { class: "brand-type-h1", "RestOS Brand Foundation" }
                p { "Светлая и тёмная темы используют одни semantic tokens и reusable primitives. Этот экран доступен только в debug-сборке." }
            }
            div { class: "brand-proof-themes",
                {proof_theme("light", "Светлая тема")}
                {proof_theme("dark", "Тёмная тема")}
            }
        }
    }
}

#[cfg(debug_assertions)]
fn proof_theme(theme: &'static str, title: &'static str) -> Element {
    rsx! {
        article { class: "brand-proof-theme", "data-brand-theme": theme,
            div { class: "brand-proof-ambient", aria_hidden: "true" }
            div { class: "brand-proof-overlay",
                div { class: "brand-proof-brand",
                    img { class: "brand-proof-mark", src: "/brand/r-icon-master.png", alt: "RestOS" }
                    div { h2 { class: "brand-type-h2", "{title}" } p { "Primary Green · Sage · Ivory · Charcoal" } }
                }
                div { class: "brand-proof-grid",
                    BrandGlass { variant: BrandGlassVariant::Primary, class: "brand-proof-card",
                        h3 { class: "brand-type-h3", "Glass / Primary" }
                        p { "Основная поверхность. Ambient остаётся деликатно видимым." }
                    }
                    BrandGlass { variant: BrandGlassVariant::Secondary, class: "brand-proof-card",
                        h3 { class: "brand-type-h3", "Glass / Secondary" }
                        p { "Более лёгкая второстепенная поверхность." }
                    }
                    BrandGlass { variant: BrandGlassVariant::Selected, class: "brand-proof-card",
                        h3 { class: "brand-type-h3", "Glass / Selected" }
                        p { "Выбранное состояние с sage tint и текстовым смыслом." }
                    }
                    BrandGlass { variant: BrandGlassVariant::Attention, class: "brand-proof-card",
                        h3 { class: "brand-type-h3", "Glass / Attention" }
                        p { "Нейтральное стекло с тёмной attention-рамкой." }
                    }
                }
                div { class: "brand-proof-row",
                    BrandButton { variant: BrandButtonVariant::Primary, "Основное действие" }
                    BrandButton { variant: BrandButtonVariant::Secondary, "Вторичное действие" }
                    BrandButton { variant: BrandButtonVariant::Destructive, "Удалить" }
                    BrandButton { variant: BrandButtonVariant::Primary, disabled: true, "Недоступно" }
                }
                div { class: "brand-proof-row", aria_label: "Статусы",
                    BrandStatus { variant: BrandStatusVariant::Excellent, label: "Отлично".to_string() }
                    BrandStatus { variant: BrandStatusVariant::Normal, label: "Норма".to_string() }
                    BrandStatus { variant: BrandStatusVariant::Attention, label: "Внимание".to_string() }
                    BrandStatus { variant: BrandStatusVariant::Critical, label: "Критично".to_string() }
                }
                BrandGlass { variant: BrandGlassVariant::Secondary, class: "brand-proof-type",
                    p { class: "brand-type-h1", "Заголовок H1" }
                    p { class: "brand-type-h2", "Заголовок H2" }
                    p { class: "brand-type-h3", "Заголовок H3" }
                    p { class: "brand-type-body", "Основной текст 16 / 24" }
                    p { class: "brand-type-secondary", "Второстепенный текст 14 / 20" }
                    p { class: "brand-type-metric", "82 / 100" }
                    small { class: "brand-proof-note", "Evolventa подключается только после поставки лицензированных packaged font assets; сейчас используется system fallback." }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relative_luminance(rgb: [u8; 3]) -> f64 {
        let channel = |value: u8| {
            let value = f64::from(value) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])
    }

    fn contrast_ratio(left: [u8; 3], right: [u8; 3]) -> f64 {
        let left = relative_luminance(left);
        let right = relative_luminance(right);
        (left.max(right) + 0.05) / (left.min(right) + 0.05)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn brand_primitive_variants_map_to_stable_semantic_classes() {
        assert_eq!(BrandGlassVariant::Primary.class_name(), "brand-glass");
        assert!(BrandGlassVariant::Attention
            .class_name()
            .contains("attention"));
        assert!(BrandButtonVariant::Destructive
            .class_name()
            .contains("destructive"));
        assert!(BrandStatusVariant::Normal.class_name().contains("normal"));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn status_variants_are_semantic_and_do_not_embed_scoring_thresholds() {
        let source = include_str!("brand_foundation.rs");
        let threshold_fragments = [
            [b'8', b'1', b'-', b'1', b'0', b'0'],
            [b'6', b'1', b'-', b'8', b'0', b' '],
            [b'4', b'1', b'-', b'6', b'0', b' '],
            [b'0', b'-', b'4', b'0', b' ', b' '],
        ];
        for bytes in threshold_fragments {
            let threshold = String::from_utf8_lossy(&bytes).trim().to_string();
            assert!(!source.contains(&threshold));
        }

        let variants = [
            BrandStatusVariant::Excellent,
            BrandStatusVariant::Normal,
            BrandStatusVariant::Attention,
            BrandStatusVariant::Critical,
        ];
        let icons: Vec<_> = variants.iter().map(|variant| variant.icon()).collect();
        let names: Vec<_> = variants
            .iter()
            .map(|variant| variant.accessible_name())
            .collect();
        assert_eq!(icons, ["✓", "–", "!", "×"]);
        assert_eq!(names.len(), 4);
        assert!(names.iter().all(|name| !name.is_empty()));
        assert!(source.contains("aria_label: accessible_label"));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn semantic_states_have_non_color_treatment_and_accessible_contrast() {
        let css = include_str!("../../assets/styling/brand_foundation.css");
        let shared = include_str!("shared.rs");
        let management = include_str!("assessment_management.rs");
        let main = include_str!("../main.rs");

        assert!(css.contains(".brand-status__icon"));
        assert!(css.contains("border-style: dashed"));
        assert!(css.contains("border-left: 5px solid var(--brand-critical)"));
        assert!(css.contains("background-image: repeating-linear-gradient"));
        assert!(css.contains("filter: saturate(0.25)"));
        assert!(shared.contains("class: \"error-container\", role: \"alert\""));
        assert!(management.contains("Необратимое действие"));
        assert!(management.contains("class: \"semantic-button-icon\""));
        assert!(main.contains("account-startup-error semantic-error-surface"));
        assert!(main.contains("role: \"alert\""));
        assert!(main.contains("semantic-state-icon--critical"));

        assert!(contrast_ratio([0x66, 0x55, 0x1f], [0xf5, 0xf3, 0xed]) >= 4.5);
        assert!(contrast_ratio([0x6b, 0x59, 0x23], [0xf5, 0xf3, 0xed]) >= 4.5);
        assert!(contrast_ratio([0xc0, 0xab, 0x55], [0x10, 0x17, 0x11]) >= 4.5);
        assert!(contrast_ratio([0x3e, 0x71, 0x35], [0xf5, 0xf3, 0xed]) >= 4.5);
        assert!(contrast_ratio([0x80, 0xb8, 0x74], [0x10, 0x17, 0x11]) >= 4.5);
        assert!(contrast_ratio([0x36, 0x57, 0x2c], [0xff, 0xff, 0xff]) >= 4.5);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn mobile_launch_is_bounded_to_mobile_and_respects_reduced_motion() {
        assert!(mobile_launch_allowed(390.0, false));
        assert!(mobile_launch_allowed(600.0, false));
        assert!(!mobile_launch_allowed(601.0, false));
        assert!(!mobile_launch_allowed(390.0, true));
        assert!(!mobile_launch_allowed(f64::NAN, false));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn mobile_launch_video_is_local_muted_inline_and_fail_open() {
        let source = include_str!("brand_foundation.rs");
        let component = source
            .split_once("pub fn MobileLaunchAnimation()")
            .and_then(|(_, source)| source.split_once("pub enum BrandGlassVariant"))
            .map(|(source, _)| source)
            .unwrap_or_default();
        assert!(component.contains("/brand/restos-launch-mobile.mp4"));
        assert!(component.contains("!mobile_launch_was_seen()"));
        assert!(component.contains("mobile_launch_allowed("));
        assert!(component.contains("mobile_launch_viewport_width()"));
        assert!(component.contains("reduced_motion_requested()"));
        assert!(component.contains("autoplay: true"));
        assert!(component.contains("muted: true"));
        assert!(component.contains("playsinline: true"));
        assert!(component.contains("onerror: move |_| playback_failed.set(true)"));
        assert!(component.contains("mobile-launch-animation__fallback"));
        assert!(!component.contains("\"Запустить\""));
        assert!(component.contains("MOBILE_LAUNCH_TIMEOUT_MS"));
        assert!(component.contains("mark_mobile_launch_seen()"));
        assert!(MOBILE_LAUNCH_TIMEOUT_MS <= 3_000);
        assert!(component.matches("visible.set(false)").count() >= 2);
        assert!(!component.contains("http:"));
        assert!(!component.contains("https:"));
        assert!(!component.contains("dangerous_inner_html"));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn production_brand_layer_unifies_fonts_buttons_and_removes_red_tones() {
        let css = include_str!("../../assets/styling/brand_foundation.css");
        let convergence = css
            .split_once("Stage 3 visual convergence")
            .map(|(_, source)| source)
            .unwrap_or_default();

        assert!(convergence.contains("--sans: var(--brand-font-family)"));
        assert!(convergence.contains("--serif: var(--brand-font-family)"));
        assert!(convergence.contains("button,"));
        assert!(convergence.contains("min-height: 44px"));
        assert!(convergence.contains(".btn-primary"));
        assert!(convergence.contains(".btn-secondary"));
        assert!(convergence.contains(".btn-danger"));
        assert!(convergence.contains("--red: #806b2d"));

        for forbidden in [
            "#f87171", "#c65f5a", "#b8493d", "#ed8d83", "#9f3f36", "#ff8f78", "#ffc29e", "#ff9c85",
            "#ff8d7a", "#ffbd9c", "#d97745",
        ] {
            assert!(!css.to_ascii_lowercase().contains(forbidden));
        }
    }
}
