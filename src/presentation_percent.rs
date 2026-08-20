//! Presentation-only ROUND_HALF_UP percentage formatting.
//!
//! Scoring and ranking continue to use exact server values.  Public surfaces
//! call this formatter instead of local float rounding variants.

fn rounded_decimal(value: &str) -> Option<i128> {
    let value = value.trim();
    if value.is_empty() || value.contains(['e', 'E']) {
        return None;
    }
    let (negative, unsigned) = match value.as_bytes().first() {
        Some(b'-') => (true, &value[1..]),
        Some(b'+') => (false, &value[1..]),
        _ => (false, value),
    };
    let mut parts = unsigned.split('.');
    let whole = parts.next()?;
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let mut rounded = whole.parse::<i128>().ok()?;
    if fraction
        .as_bytes()
        .first()
        .is_some_and(|digit| *digit >= b'5')
    {
        rounded = rounded.checked_add(1)?;
    }
    Some(if negative { -rounded } else { rounded })
}

pub fn format_percent(value: &str) -> Option<String> {
    rounded_decimal(value).map(|rounded| format!("{rounded}%"))
}

pub fn format_percent_number(value: f64) -> Option<String> {
    value
        .is_finite()
        .then(|| value.to_string())
        .and_then(|value| format_percent(&value))
}

pub fn format_percentage_points(value: &str) -> Option<String> {
    rounded_decimal(value).map(|rounded| {
        if rounded > 0 {
            format!("+{rounded} п.п.")
        } else {
            format!("{rounded} п.п.")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn integer_percentage_golden_contract_is_round_half_up() {
        for (value, expected) in [
            ("87.49", "87%"),
            ("87.50", "88%"),
            ("99.50", "100%"),
            ("0.49", "0%"),
            ("0", "0%"),
            ("100.0000", "100%"),
        ] {
            assert_eq!(format_percent(value).as_deref(), Some(expected));
        }
    }

    #[wasm_bindgen_test]
    fn percentage_points_use_the_same_decimal_contract() {
        assert_eq!(format_percentage_points("3.5").as_deref(), Some("+4 п.п."));
        assert_eq!(format_percentage_points("-3.5").as_deref(), Some("-4 п.п."));
        assert_eq!(format_percentage_points("0.49").as_deref(), Some("0 п.п."));
        assert!(format_percent("NaN").is_none());
        assert!(format_percent("1e2").is_none());
    }
}
