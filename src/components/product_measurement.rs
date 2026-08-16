//! Dynamic product taste/speed measurement UI; nothing is persisted in browser storage.

use dioxus::prelude::*;
use uuid::Uuid;
use wasm_bindgen::{JsCast, JsValue};

use crate::{
    account_session::{AccountSessionAdapter, AccountSessionState},
    product_measurement_api::{
        ProductItem, ProductMeasurement, ProductMeasurementApiClient, ProductMeasurementApiError,
    },
    workforce_api::{WorkforceApiClient, WorkforceVenue},
};

fn safe_error(value: &ProductMeasurementApiError) -> &'static str {
    match value {
        ProductMeasurementApiError::AuthenticationRequired => "Сессия недоступна.",
        ProductMeasurementApiError::PermissionDenied => "Доступ к замерам отозван.",
        ProductMeasurementApiError::Conflict => "Замер изменился. Создайте новый замер.",
        ProductMeasurementApiError::NetworkUnavailable => "Нет связи. Повторите вручную.",
        _ => "Не удалось выполнить действие.",
    }
}

fn context(
    session: &AccountSessionAdapter,
) -> Option<(crate::account_api::AccountAccessToken, Uuid)> {
    let AccountSessionState::Authenticated(value) = session.state() else {
        return None;
    };
    Some((value.access_token, value.selected_company?.0))
}

fn browser_request_id() -> Option<Uuid> {
    let crypto = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("crypto")).ok()?;
    if crypto.is_null() || crypto.is_undefined() {
        return None;
    }
    let random_uuid = js_sys::Reflect::get(&crypto, &JsValue::from_str("randomUUID")).ok()?;
    let function = random_uuid.dyn_into::<js_sys::Function>().ok()?;
    let value = function.call0(&crypto).ok()?.as_string()?;
    Uuid::parse_str(&value).ok()
}

#[component]
pub fn ProductMeasurementPanel() -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<ProductMeasurementApiClient>();
    let workforce = use_context::<WorkforceApiClient>();
    let mut measurement = use_signal(|| None::<ProductMeasurement>);
    let mut selected_venue = use_signal(String::new);
    let mut position_type = use_signal(|| "dish".to_string());
    let mut position_name = use_signal(String::new);
    let mut taste = use_signal(|| "3".to_string());
    let mut appearance = use_signal(|| "3".to_string());
    let mut output = use_signal(|| "3".to_string());
    let mut ticket_score = use_signal(|| "3".to_string());
    let mut planned_quantity = use_signal(String::new);
    let mut actual_quantity = use_signal(String::new);
    let mut quantity_unit = use_signal(|| "g".to_string());
    let mut planned_seconds = use_signal(String::new);
    let mut actual_seconds = use_signal(String::new);
    let mut comment = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut message = use_signal(|| None::<String>);
    let mut create_request_id = use_signal(|| None::<Uuid>);

    let venue_session = session.clone();
    let venues = use_resource(move || {
        let state = context(&venue_session);
        let workforce = workforce.clone();
        async move {
            let Some((token, company_id)) = state else {
                return Vec::<WorkforceVenue>::new();
            };
            workforce
                .venues(&token, company_id)
                .await
                .unwrap_or_default()
        }
    });

    let create_session = session.clone();
    let create_api = api.clone();
    let replace_session = session.clone();
    let replace_api = api.clone();
    let remove_session = session.clone();
    let remove_api = api.clone();
    let complete_session = session.clone();
    let complete_api = api.clone();

    rsx! {
        section { class:"product-measurement-panel", aria_labelledby:"product-measurement-title",
            div { class:"metric-card-head",
                div { h2 { id:"product-measurement-title", "Замер вкуса и скорости" } p { class:"management-muted", "Добавляйте только фактически проверенные позиции." } }
                if measurement().is_none() {
                    div { class:"product-create",
                        select { aria_label:"Ресторан", value:"{selected_venue}", onchange:move |event| { selected_venue.set(event.value()); create_request_id.set(None); },
                            option { value:"", "Выберите ресторан" }
                            if let Some(values) = venues() { for venue in values.iter() { option { value:"{venue.venue_id}", "{venue.name}" } } }
                        }
                        button { class:"btn-primary", r#type:"button", disabled:busy() || selected_venue().is_empty(), onclick:move |_| {
                            let Ok(venue_id) = Uuid::parse_str(&selected_venue()) else { return; };
                            let Some((token, company_id)) = context(&create_session) else { return; };
                            let request_id = match create_request_id() {
                                Some(value) => value,
                                None => {
                                    let Some(value) = browser_request_id() else {
                                        message.set(Some("Не удалось подготовить безопасный запрос. Обновите страницу.".into()));
                                        return;
                                    };
                                    create_request_id.set(Some(value));
                                    value
                                }
                            };
                            busy.set(true); message.set(None); let api=create_api.clone();
                            spawn(async move { let result=api.create(&token,company_id,request_id,venue_id).await; busy.set(false); match result { Ok(value)=>{create_request_id.set(None);measurement.set(Some(value));}, Err(problem)=>message.set(Some(safe_error(&problem).into())) } });
                        }, "Начать замер" }
                    }
                }
            }
            div { class:"management-live", role:"status", aria_live:"polite", if let Some(value)=message() { "{value}" } }
            if let Some(current) = measurement() {
                if current.status == "draft" {
                    div { class:"product-entry-grid",
                        label { "Тип", select { value:"{position_type}", onchange:move |e| { let v=e.value(); position_type.set(v.clone()); quantity_unit.set(if v=="drink" {"ml".into()} else {"g".into()}); }, option { value:"dish", "Блюдо" } option { value:"drink", "Напиток" } } }
                        label { "Название", input { value:"{position_name}", maxlength:"255", oninput:move |e| position_name.set(e.value()) } }
                        label { "Вкус (0–3)", input { r#type:"number", min:"0", max:"3", step:"0.1", value:"{taste}", oninput:move |e| taste.set(e.value()) } }
                        label { "Внешний вид (0–3)", input { r#type:"number", min:"0", max:"3", step:"0.1", value:"{appearance}", oninput:move |e| appearance.set(e.value()) } }
                        label { "Соответствие веса/объёма (0–3)", input { r#type:"number", min:"0", max:"3", step:"0.1", value:"{output}", oninput:move |e| output.set(e.value()) } }
                        label { "Ticket Time (0–3)", input { r#type:"number", min:"0", max:"3", step:"0.1", value:"{ticket_score}", oninput:move |e| ticket_score.set(e.value()) } }
                        label { "Плановый вес/объём", input { r#type:"number", min:"0", step:"0.1", value:"{planned_quantity}", oninput:move |e| planned_quantity.set(e.value()) } }
                        label { "Фактический вес/объём", input { r#type:"number", min:"0", step:"0.1", value:"{actual_quantity}", oninput:move |e| actual_quantity.set(e.value()) } }
                        label { "Ticket Time план, сек.", input { r#type:"number", min:"0", max:"86400", value:"{planned_seconds}", oninput:move |e| planned_seconds.set(e.value()) } }
                        label { "Ticket Time факт, сек.", input { r#type:"number", min:"0", max:"86400", value:"{actual_seconds}", oninput:move |e| actual_seconds.set(e.value()) } }
                        label { class:"product-comment", "Комментарий", textarea { maxlength:"10000", value:"{comment}", oninput:move |e| comment.set(e.value()) } }
                    }
                    button { class:"btn-secondary", r#type:"button", disabled:busy(), onclick:move |_| {
                        let Some((token,company_id))=context(&replace_session) else { return; };
                        let Ok(taste_value)=taste().parse() else { message.set(Some("Проверьте оценки.".into())); return; };
                        let Ok(appearance_value)=appearance().parse() else { message.set(Some("Проверьте оценки.".into())); return; };
                        let Ok(output_value)=output().parse() else { message.set(Some("Проверьте оценки.".into())); return; };
                        let Ok(ticket_value)=ticket_score().parse() else { message.set(Some("Проверьте оценки.".into())); return; };
                        let Ok(plan_quantity)=planned_quantity().parse() else { message.set(Some("Укажите плановый выход.".into())); return; };
                        let Ok(fact_quantity)=actual_quantity().parse() else { message.set(Some("Укажите фактический выход.".into())); return; };
                        let Ok(plan_seconds)=planned_seconds().parse() else { message.set(Some("Укажите плановый Ticket Time.".into())); return; };
                        let Ok(fact_seconds)=actual_seconds().parse() else { message.set(Some("Укажите фактический Ticket Time.".into())); return; };
                        if position_name().trim().is_empty() { message.set(Some("Укажите название позиции.".into())); return; }
                        let item=ProductItem { position_type:position_type(), position_name:position_name().trim().into(), taste_score:taste_value, appearance_score:appearance_value, output_score:output_value, ticket_time_score:ticket_value, planned_quantity:plan_quantity, actual_quantity:fact_quantity, quantity_unit:quantity_unit(), planned_ticket_seconds:plan_seconds, actual_ticket_seconds:fact_seconds, comment:(!comment().trim().is_empty()).then(|| comment().trim().into()) };
                        let Some(snapshot)=measurement() else { return; };
                        let mut items=snapshot.items.clone(); items.push(item); busy.set(true); message.set(None); let api=replace_api.clone();
                        spawn(async move { let result=api.replace(&token,company_id,&snapshot,&items).await; busy.set(false); match result { Ok(value)=>{measurement.set(Some(value));position_name.set(String::new());comment.set(String::new());}, Err(problem)=>message.set(Some(safe_error(&problem).into())) } });
                    }, "Добавить позицию" }
                    div { class:"product-items",
                        for (index,item) in current.items.iter().enumerate() {
                            {
                                let remove_session = remove_session.clone();
                                let remove_api = remove_api.clone();
                                rsx! {
                                    article {
                                        div {
                                            strong { "{index + 1}. {item.position_name}" }
                                            small { "Вкус {item.taste_score} · вид {item.appearance_score} · выход {item.output_score} · скорость {item.ticket_time_score}" }
                                        }
                                        button { class:"btn-ghost", r#type:"button", disabled:busy(), onclick:move |_| {
                                            let Some((token,company_id))=context(&remove_session) else { return; };
                                            let Some(snapshot)=measurement() else { return; };
                                            let items=snapshot.items.iter().enumerate().filter(|(position,_)| *position != index).map(|(_,value)| value.clone()).collect::<Vec<_>>();
                                            busy.set(true); message.set(None); let api=remove_api.clone();
                                            spawn(async move { let result=api.replace(&token,company_id,&snapshot,&items).await; busy.set(false); match result { Ok(value)=>measurement.set(Some(value)), Err(problem)=>message.set(Some(safe_error(&problem).into())) } });
                                        }, "Удалить" }
                                    }
                                }
                            }
                        }
                    }
                    button { class:"btn-primary", r#type:"button", disabled:busy() || current.items.is_empty(), onclick:move |_| {
                        let Some((token,company_id))=context(&complete_session) else { return; };
                        let Some(snapshot)=measurement() else { return; };
                        busy.set(true); message.set(None); let api=complete_api.clone();
                        spawn(async move { let result=api.complete(&token,company_id,&snapshot).await; busy.set(false); match result { Ok(value)=>measurement.set(Some(value)), Err(problem)=>message.set(Some(safe_error(&problem).into())) } });
                    }, "Завершить замер" }
                } else if let Some(result)=current.result.as_ref() {
                    div {
                        class:"product-result",
                        strong { "Вкус: {result.taste.score_percent}%" }
                        strong { "Скорость: {result.speed.score_percent}%" }
                        strong { "Итог замера: {result.overall.score_percent}%" }
                        span { "Позиций: {result.item_count}" }
                        ul { class: "product-position-results",
                            for value in result.items.iter() {
                                li {
                                    if let Some(item) = current.items.get(value.position_index) {
                                        strong { "{item.position_name}: " }
                                    } else {
                                        strong { "Позиция {value.position_index + 1}: " }
                                    }
                                    "вкус {value.taste.score_percent}% · скорость {value.speed.score_percent}% · итог {value.overall.score_percent}%"
                                }
                            }
                        }
                        button {
                            class:"btn-secondary",
                            r#type:"button",
                            onclick:move |_| {
                                measurement.set(None);
                                create_request_id.set(None);
                                selected_venue.set(String::new());
                                position_name.set(String::new());
                                comment.set(String::new());
                                message.set(None);
                            },
                            "Новый замер"
                        }
                    }
                }
            }
        }
    }
}
