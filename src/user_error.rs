//! Сообщения об ошибках для пользователя: без сырых TypeError / «load failed» и т.п.

fn has_cyrillic(s: &str) -> bool {
    s.chars()
        .any(|c| matches!(c, '\u{0400}'..='\u{04FF}' | '\u{0500}'..='\u{052F}'))
}

fn looks_like_html(s: &str) -> bool {
    let t = s.trim_start();
    t.starts_with('<') || t.to_lowercase().starts_with("<!doctype")
}

fn looks_like_technical_junk(s: &str) -> bool {
    let l = s.to_lowercase();
    l.contains("typeerror")
        || l.contains("referenceerror")
        || l.contains("syntaxerror")
        || l.contains("rangeerror")
        || l.contains("aggregateerror")
        || l.contains("wasm ")
        || l.contains("uncaught")
        || l.contains("runtime.lasterror")
        || l.contains("failed to fetch")
        || l.contains("load failed")
        || l.contains("networkerror")
        || l.contains("network request failed")
        || l.contains("err_connection")
        || l.contains("err_internet")
        || l.contains("err_name_not_resolved")
        || l.contains("serde_json")
        || l.contains("error decoding")
}

fn should_show_api_detail(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty()
        && !looks_like_html(t)
        && has_cyrillic(t)
        && !looks_like_technical_junk(t)
        && t.len() < 600
}

fn humanize_english_and_junk(s: &str) -> String {
    let l = s.to_lowercase();
    if l.contains("load failed")
        || l.contains("failed to fetch")
        || l.contains("networkerror")
        || l.contains("network request failed")
        || l.contains("a network error occurred")
        || l.contains("fetch error")
    {
        return "Не удалось связаться с сервером. Проверьте подключение к интернету и попробуйте снова."
            .to_string();
    }
    if l.contains("aborted") || l.contains("abort") {
        return "Запрос был прерван. Попробуйте ещё раз.".to_string();
    }
    if l.contains("timeout") || l.contains("timed out") {
        return "Превышено время ожидания ответа. Попробуйте позже.".to_string();
    }
    if l.contains("serde")
        || l.contains("invalid json")
        || (l.contains("expected") && l.contains("json"))
    {
        return "Не удалось разобрать ответ сервера. Обновите страницу или попробуйте позже."
            .to_string();
    }
    if l.contains("401") || l.contains("unauthorized") {
        return "Сессия истекла или вход не выполнен. Выйдите и войдите снова.".to_string();
    }
    if l.contains("403") || l.contains("forbidden") || l.contains("access denied") {
        return "Недостаточно прав для этого действия.".to_string();
    }
    if l.contains("404") || l.contains("not found") {
        return "Запрашиваемые данные не найдены.".to_string();
    }
    if l.contains("409") || l.contains("conflict") {
        return "Данные конфликтуют с уже существующими. Обновите страницу.".to_string();
    }
    if l.contains("422") || l.contains("unprocessable") {
        return "Отправленные данные не прошли проверку. Проверьте поля формы.".to_string();
    }
    if l.contains("429") || l.contains("too many requests") {
        return "Слишком много запросов. Подождите немного и повторите.".to_string();
    }
    if l.contains("502")
        || l.contains("503")
        || l.contains("504")
        || l.contains("bad gateway")
        || l.contains("service unavailable")
        || l.contains("gateway timeout")
    {
        return "Сервер временно недоступен. Попробуйте через несколько минут.".to_string();
    }
    if l.contains("unknown error") {
        return "Не удалось выполнить операцию. Попробуйте позже.".to_string();
    }
    if l.contains("no window") || l.contains("invalid response") {
        return "Приложение не смогло открыть соединение. Обновите страницу.".to_string();
    }
    if l.contains("invalid credentials")
        || l.contains("incorrect password")
        || l.contains("wrong password")
        || l.contains("bad credentials")
    {
        return "Неверный логин или пароль.".to_string();
    }
    if l.contains("bad request") || l.contains("invalid request") {
        return "Некорректный запрос. Проверьте введённые данные.".to_string();
    }
    if looks_like_technical_junk(s) || s.len() > 800 {
        return "Произошла техническая ошибка. Попробуйте позже или обновите страницу.".to_string();
    }
    "Произошла ошибка. Попробуйте ещё раз или обновите страницу.".to_string()
}

fn status_fallback(status: u16) -> Option<&'static str> {
    match status {
        400 => Some("Некорректный запрос. Проверьте данные."),
        401 => Some("Сессия истекла или вход не выполнен. Войдите снова."),
        403 => Some("Недостаточно прав для этого действия."),
        404 => Some("Запрашиваемые данные не найдены."),
        408 | 504 => Some("Превышено время ожидания ответа сервера."),
        409 => Some("Конфликт данных. Обновите страницу."),
        422 => Some("Данные не прошли проверку. Проверьте введённые значения."),
        429 => Some("Слишком много запросов. Подождите и повторите."),
        500..=599 => Some("На сервере произошла ошибка. Попробуйте позже."),
        _ => None,
    }
}

/// Ошибки сети / сериализации из клиента (gloo, serde и т.д.).
pub fn from_transport_error(e: impl std::fmt::Display) -> String {
    let s = e.to_string();
    let t = s.trim();
    if t.is_empty() {
        return "Не удалось выполнить операцию. Попробуйте позже.".to_string();
    }
    if should_show_api_detail(t) {
        return t.to_string();
    }
    humanize_english_and_junk(t)
}

/// Тело ответа HTTP + код статуса (после разбора ApiError при наличии).
pub fn from_http_status_and_detail(status: u16, detail: &str) -> String {
    let d = detail.trim();
    if should_show_api_detail(d) {
        return d.to_string();
    }
    let generic = "Произошла ошибка. Попробуйте ещё раз или обновите страницу.";
    let from_body = if d.is_empty() || looks_like_html(d) {
        None
    } else {
        let h = humanize_english_and_junk(d);
        if h != generic {
            Some(h)
        } else {
            None
        }
    };
    if let Some(h) = from_body {
        return h;
    }
    if let Some(m) = status_fallback(status) {
        return m.to_string();
    }
    if !d.is_empty() && !looks_like_html(d) {
        return humanize_english_and_junk(d);
    }
    "Не удалось выполнить операцию. Попробуйте позже.".to_string()
}

/// Любая строка, которую показываем в UI (в т.ч. из JS fatal handler).
pub fn for_ui_message(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return "Произошла ошибка. Попробуйте обновить страницу.".to_string();
    }
    if should_show_api_detail(t) {
        return t.to_string();
    }
    humanize_english_and_junk(t)
}
