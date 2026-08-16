//! Shared status views — loading spinner, error, success, skeletons.

use dioxus::prelude::*;

/// Full-screen shimmer gate while the session `/me` check has no cached payload yet.
#[component]
pub fn SessionGateSkeleton() -> Element {
    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                div { class: "pad", style: "padding-top: max(10px, var(--safe-top));",
                    div { style: "display:flex; justify-content: space-between; align-items: flex-start; gap: 14px;",
                        div { style: "flex:1; min-width:0;",
                            div { class: "sk sk-line sk-h-sm sk-w-35" }
                            div { class: "sk sk-line sk-h-md sk-w-70 sk-mt" }
                            div { class: "sk sk-line sk-h-sm sk-w-50 sk-mt-sm" }
                        }
                        div { class: "sk sk-av" }
                    }
                    div { class: "stats-row", style: "margin-top: 22px;",
                        for _ in 0..3usize {
                            div { class: "stat-card sk-stat",
                                div { class: "sk sk-line sk-h-xs sk-w-55" }
                                div { class: "sk sk-line sk-h-lg sk-w-40 sk-mt-md" }
                            }
                        }
                    }
                    div { class: "pad", style: "margin-top: 26px;",
                        div { class: "sk sk-line sk-h-xs sk-w-30" }
                        div { class: "actions-grid", style: "margin-top: 12px;",
                            for _ in 0..4usize {
                                div { class: "action-card sk-action",
                                    div { class: "sk sk-line sk-h-icon sk-w-icon sk-br-md" }
                                    div { class: "sk sk-line sk-h-sm sk-w-55 sk-mt-md" }
                                    div { class: "sk sk-line sk-h-xs sk-w-80 sk-mt-sm" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Home dashboard placeholder (stats + actions + list strip).
#[component]
pub fn HomeDashboardSkeleton() -> Element {
    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                div { style: "display:flex; align-items:center; justify-content:space-between; padding:16px 20px 0; gap: 14px;",
                    div { style: "flex:1; min-width:0;",
                        div { class: "sk sk-line sk-h-xs sk-w-30" }
                        div { class: "sk sk-line sk-h-md sk-w-75 sk-mt-sm" }
                        div { class: "sk sk-line sk-h-sm sk-w-50 sk-mt-sm" }
                    }
                    div { class: "sk sk-av" }
                }
                div { class: "stats-row", style: "margin-top:16px;",
                    for _ in 0..3usize {
                        div { class: "stat-card sk-stat",
                            div { class: "sk sk-line sk-h-xs sk-w-50" }
                            div { class: "sk sk-line sk-h-xl sk-w-35 sk-mt-md" }
                            div { class: "sk sk-line sk-h-xs sk-w-45 sk-mt-sm" }
                        }
                    }
                }
                div { class: "pad", style: "margin-top:24px;",
                    div { class: "sk sk-line sk-h-xs sk-w-40" }
                    div { class: "actions-grid", style: "margin-top:10px;",
                        for _ in 0..4usize {
                            div { class: "action-card sk-action",
                                div { class: "sk sk-line sk-h-icon sk-w-icon sk-br-md" }
                                div { class: "sk sk-line sk-h-sm sk-w-50 sk-mt-md" }
                                div { class: "sk sk-line sk-h-xs sk-w-85 sk-mt-sm" }
                            }
                        }
                    }
                }
                div { class: "pad", style: "margin-top:24px;",
                    div { class: "sk sk-line sk-h-xs sk-w-45" }
                    div { class: "card", style: "margin-top:10px; padding: 8px 0;",
                        for _ in 0..4usize {
                            div { class: "list-row sk-list-row",
                                div { class: "sk sk-line sk-h-icon sk-w-icon sk-br-full" }
                                div { style: "flex:1; min-width:0;",
                                    div { class: "sk sk-line sk-h-sm sk-w-60" }
                                    div { class: "sk sk-line sk-h-xs sk-w-full sk-mt-sm" }
                                }
                                div { class: "sk sk-line sk-h-sm sk-w-15" }
                            }
                        }
                    }
                }
                div { style: "height:20px;" }
            }
        }
    }
}

#[component]
pub fn AnalyticsPageSkeleton() -> Element {
    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                div { class: "page-header",
                    div {
                        div { class: "sk sk-line sk-h-md sk-w-55" }
                        div { class: "sk sk-line sk-h-sm sk-w-80 sk-mt-sm" }
                    }
                }
                div { class: "stats-row",
                    for _ in 0..3usize {
                        div { class: "stat-card sk-stat",
                            div { class: "sk sk-line sk-h-xs sk-w-50" }
                            div { class: "sk sk-line sk-h-xl sk-w-30 sk-mt-md" }
                        }
                    }
                }
                div { class: "tab-bar", style: "margin: 16px 20px 0; display:flex; gap:8px;",
                    div { class: "sk sk-line sk-h-tab sk-w-tab sk-br-pill" }
                    div { class: "sk sk-line sk-h-tab sk-w-tab sk-br-pill" }
                }
                div { class: "pad", style: "margin-top:16px;",
                    div { class: "card", style: "padding: 8px 0;",
                        for _ in 0..5usize {
                            div { class: "list-row sk-list-row",
                                div { class: "sk sk-line sk-h-icon sk-w-icon sk-br-full" }
                                div { style: "flex:1;",
                                    div { class: "sk sk-line sk-h-sm sk-w-65" }
                                    div { class: "sk sk-line sk-h-xs sk-w-40 sk-mt-sm" }
                                    div { class: "sk sk-line sk-h-progress sk-w-full sk-mt-sm sk-br-sm" }
                                }
                                div { class: "sk sk-line sk-h-sm sk-w-15" }
                            }
                        }
                    }
                }
                div { style: "height:20px;" }
            }
        }
    }
}

#[component]
pub fn EmployeesPageSkeleton() -> Element {
    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                div { class: "page-header page-header-spacious",
                    div {
                        div { class: "sk sk-line sk-h-xs sk-w-30" }
                        div { class: "sk sk-line sk-h-lg sk-w-50 sk-mt-sm" }
                        div { class: "sk sk-line sk-h-sm sk-w-40 sk-mt-sm" }
                    }
                }
                div { class: "pad", style: "margin-top: 4px;",
                    div { class: "sk sk-line sk-h-search sk-w-full sk-br-lg" }
                }
                div { class: "filter-row filter-row-tight pad", style: "margin-top: 10px;",
                    for _ in 0..3usize {
                        div { class: "sk sk-line sk-h-chip sk-w-chip sk-br-pill" }
                    }
                }
                div { class: "list-section", style: "margin-top: 8px; padding: 0 20px;",
                    for _ in 0..7usize {
                        div { class: "card list-emp-sk-row",
                            div { style: "display:flex; align-items:center; gap: 12px; padding: 14px 16px;",
                                div { class: "sk sk-line sk-av sk-br-full" }
                                div { style: "flex:1; min-width:0;",
                                    div { class: "sk sk-line sk-h-sm sk-w-55" }
                                    div { class: "sk sk-line sk-h-xs sk-w-40 sk-mt-sm" }
                                }
                                div { class: "sk sk-line sk-h-chip sk-w-badge sk-br-pill" }
                            }
                        }
                    }
                }
                div { style: "height:20px;" }
            }
        }
    }
}

#[component]
pub fn EvaluationsOverviewSkeleton() -> Element {
    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                div { class: "page-header page-header-spacious",
                    div {
                        div { class: "sk sk-line sk-h-xs sk-w-35" }
                        div { class: "sk sk-line sk-h-lg sk-w-45 sk-mt-sm" }
                    }
                }
                div { class: "pad", style: "margin-top: 8px;",
                    div { class: "sk sk-line sk-h-search sk-w-full sk-br-lg" }
                }
                div { class: "filter-row pad", style: "margin-top: 10px;",
                    for _ in 0..3usize {
                        div { class: "sk sk-line sk-h-chip sk-w-chip sk-br-pill" }
                    }
                }
                for _ in 0..5usize {
                    div { class: "eval-card sk-eval-card",
                        div { style: "display:flex; align-items:center; gap: 10px;",
                            div { class: "sk sk-line sk-h-icon sk-w-icon sk-br-full" }
                            div { style: "flex:1;",
                                div { class: "sk sk-line sk-h-sm sk-w-60" }
                                div { class: "sk sk-line sk-h-xs sk-w-70 sk-mt-sm" }
                            }
                            div { class: "sk sk-line sk-h-chip sk-w-badge sk-br-pill" }
                        }
                        div { class: "sk sk-line sk-h-progress sk-w-full sk-mt-md sk-br-sm" }
                    }
                }
                div { class: "pad", style: "margin-top: 18px;",
                    div { class: "sk sk-line sk-h-xs sk-w-35" }
                    for _ in 0..4usize {
                        div { class: "config-entry-card sk-config-card",
                            div { class: "sk sk-line sk-h-icon sk-w-icon sk-br-md" }
                            div { style: "flex:1;",
                                div { class: "sk sk-line sk-h-sm sk-w-50" }
                                div { class: "sk sk-line sk-h-xs sk-w-85 sk-mt-sm" }
                            }
                            div { class: "sk sk-line sk-h-xs sk-w-chevron" }
                        }
                    }
                }
                div { style: "height:20px;" }
            }
        }
    }
}

#[component]
pub fn AccountPageSkeleton() -> Element {
    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                div { class: "form-header-bar",
                    div { class: "sk sk-line sk-h-md sk-w-35" }
                }
                div { class: "pad", style: "padding-top: 4px;",
                    div { class: "card card-glass",
                        div { style: "display:flex; justify-content: space-between; align-items:center;",
                            div { class: "sk sk-line sk-h-sm sk-w-30" }
                            div { class: "sk sk-line sk-h-chip sk-w-badge sk-br-pill" }
                        }
                        div { style: "margin-top: 14px; display:flex; flex-direction:column; gap: 12px;",
                            for _ in 0..3usize {
                                div { style: "display:flex; justify-content: space-between; gap: 12px;",
                                    div { class: "sk sk-line sk-h-xs sk-w-20" }
                                    div { class: "sk sk-line sk-h-sm sk-w-45" }
                                }
                            }
                        }
                    }
                    div { class: "card", style: "margin-top:14px; padding: 16px;",
                        div { class: "sk sk-line sk-h-sm sk-w-70" }
                        div { class: "sk sk-line sk-h-xs sk-w-full sk-mt-sm" }
                        div { class: "sk sk-line sk-h-xs sk-w-full sk-mt-sm" }
                        for _ in 0..3usize {
                            div { class: "sk sk-line sk-h-field sk-w-full sk-mt-md sk-br-md" }
                        }
                        div { class: "sk sk-line sk-h-btn sk-w-full sk-mt-lg sk-br-md" }
                    }
                }
            }
        }
    }
}

/// Criteria / sets / types config list header + rows.
#[component]
pub fn ConfigListPageSkeleton() -> Element {
    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                div { class: "pad", style: "padding-top: 12px;",
                    div { style: "display:flex; align-items:center; justify-content: space-between; gap: 12px;",
                        div { class: "sk sk-line sk-h-icon sk-w-icon sk-br-md" }
                        div { style: "flex:1;",
                            div { class: "sk sk-line sk-h-xs sk-w-40" }
                            div { class: "sk sk-line sk-h-md sk-w-55 sk-mt-sm" }
                        }
                        div { class: "sk sk-line sk-h-icon sk-w-icon sk-br-md" }
                    }
                }
                div { class: "pad config-search-wrap", style: "margin-top: 8px;",
                    div { class: "sk sk-line sk-h-search sk-w-full sk-br-lg" }
                }
                div { class: "filter-row filter-row-tight pad", style: "margin-top: 8px;",
                    for _ in 0..4usize {
                        div { class: "sk sk-line sk-h-chip sk-w-chip-lg sk-br-pill" }
                    }
                }
                div { class: "list-section config-list-section",
                    for _ in 0..6usize {
                        div { class: "card sk-config-row",
                            div { style: "display:flex; gap: 12px; padding: 14px 16px; align-items: flex-start;",
                                div { class: "sk sk-line sk-h-icon sk-w-icon sk-br-md" }
                                div { style: "flex:1; min-width:0;",
                                    div { class: "sk sk-line sk-h-sm sk-w-70" }
                                    div { class: "sk sk-line sk-h-xs sk-w-50 sk-mt-sm" }
                                    div { class: "sk sk-line sk-h-xs sk-w-90 sk-mt-sm" }
                                }
                            }
                        }
                    }
                }
                div { style: "height:20px;" }
            }
        }
    }
}

#[component]
pub fn EvaluationDetailSkeleton() -> Element {
    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll", style: "padding-bottom: 28px;",
                div { class: "page-header page-header-spacious",
                    style: "display: flex; align-items: center; gap: 10px;",
                    div { class: "sk sk-line sk-h-icon sk-w-icon sk-br-md" }
                    div {
                        div { class: "sk sk-line sk-h-xs sk-w-35" }
                        div { class: "sk sk-line sk-h-md sk-w-40 sk-mt-sm" }
                    }
                }
                div { class: "card", style: "margin: 12px 20px 0; padding: 16px;",
                    div { style: "display:flex; gap: 12px;",
                        div { class: "sk sk-line sk-h-lg sk-w-lg sk-br-full" }
                        div { style: "flex:1;",
                            div { class: "sk sk-line sk-h-sm sk-w-60" }
                            div { class: "sk sk-line sk-h-xs sk-w-45 sk-mt-sm" }
                        }
                    }
                    for _ in 0..6usize {
                        div { style: "display:flex; justify-content: space-between; margin-top: 12px; gap: 10px;",
                            div { class: "sk sk-line sk-h-xs sk-w-35" }
                            div { class: "sk sk-line sk-h-sm sk-w-40" }
                        }
                    }
                }
                div { class: "sk sk-line sk-h-sm sk-w-50 sk-mt-lg", style: "margin-left: 20px;" }
                for _ in 0..4usize {
                    div { class: "card", style: "margin: 10px 20px 0; padding: 12px 14px;",
                        div { style: "display:flex; justify-content: space-between;",
                            div { class: "sk sk-line sk-h-sm sk-w-50" }
                            div { class: "sk sk-line sk-h-sm sk-w-15" }
                        }
                        div { class: "sk sk-line sk-h-xs sk-w-full sk-mt-sm" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn FormPrepareSkeleton() -> Element {
    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                div { style: "display:flex;flex-direction:column;align-items:stretch;min-height:280px;padding:20px;gap:14px;",
                    div { class: "sk sk-line sk-h-md sk-w-50" }
                    div { class: "sk sk-line sk-h-xs sk-w-full" }
                    div { class: "sk sk-line sk-h-field sk-w-full sk-br-md" }
                    div { class: "sk sk-line sk-h-field sk-w-full sk-br-md" }
                    div { class: "sk sk-line sk-h-btn sk-w-full sk-br-md", style: "margin-top: 8px;" }
                }
            }
        }
    }
}

#[component]
pub fn LoadingView(message: String) -> Element {
    rsx! {
        div { class: "loading-container",
            div { class: "spinner-dots",
                div { class: "dot dot-1" }
                div { class: "dot dot-2" }
                div { class: "dot dot-3" }
            }
            p { class: "loading-text", "{message}" }
        }
    }
}

#[component]
pub fn ErrorView(message: String) -> Element {
    rsx! {
        div { class: "error-container", role: "alert",
            div { class: "error-icon", aria_hidden: "true", "!" }
            h2 { style: "font-size:18px; font-weight:500; color:var(--text);", "Ошибка" }
            p { style: "font-size:14px; color:var(--text2); font-weight:300;", "{message}" }
            button {
                class: "btn-retry",
                onclick: |_| {
                    let _ = web_sys::window()
                        .and_then(|w| w.location().reload().ok());
                },
                "Попробовать снова"
            }
        }
    }
}

#[component]
pub fn SuccessView(title: String, subtitle: String) -> Element {
    rsx! {
        div { class: "success-container",
            div { class: "success-icon", "✓" }
            h2 { style: "font-size:20px; font-weight:500; color:var(--green);", "{title}" }
            p { style: "font-size:14px; color:var(--text2); font-weight:300;", "{subtitle}" }
        }
    }
}
