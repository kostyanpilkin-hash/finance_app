//! BSB Bank exchange rates — the unofficial endpoint behind bsb.by's rates page
//! (no key, no SLA). Callers must fall back to NBRB when it fails.

use finance_core::fx::{Quote, RateKind};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::Error;

pub const URL: &str = "https://mobile.bsb.by/api/v1/free-zone-management/exchange-rates/rates";

/// API `type` for a rate kind; NBRB is not fetched from here.
pub fn api_type(kind: RateKind) -> Option<&'static str> {
    match kind {
        RateKind::BsbApp => Some("CARD_ONLINE"),
        RateKind::BsbCard => Some("CARD"),
        RateKind::Nbrb => None,
    }
}

/// POST body: the rates in effect at `at_ms` (unix ms). Department 7 is the
/// site default; it only matters for cash rates.
pub fn request_body(kind: RateKind, at_ms: i64) -> Value {
    json!({"type": api_type(kind).unwrap_or("CARD_ONLINE"), "bankDepartmentId": 7, "period": at_ms})
}

/// One rate set: valid from `from_ms` until `next_ms` (None = still current).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub from_ms: i64,
    pub next_ms: Option<i64>,
    pub quotes: Vec<(String, Quote)>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Resp {
    from_time: i64,
    next_time: Option<i64>,
    rates: Vec<Row>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    buy_currency_name: String,
    sell_currency_name: String,
    scaled_buy_amount: String,
    scaled_sell_amount: String,
    buy_currency_scale: i64,
}

/// Decimal string for `scale` units → BYN per 1 unit ×1e6, integer-only,
/// rounded half up. `("3.4753", 100)` → `34_753`.
pub fn parse_scaled(s: &str, scale: i64) -> Option<i64> {
    if scale <= 0 {
        return None;
    }
    let s = s.trim();
    let (int, frac) = s.split_once('.').unwrap_or((s, ""));
    if int.is_empty()
        || frac.len() > 8
        || !int.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let mantissa: i128 = format!("{int}{frac}").parse().ok()?;
    let den = 10i128.pow(frac.len() as u32) * scale as i128;
    let v = (mantissa * 1_000_000 + den / 2) / den;
    i64::try_from(v).ok()
}

/// Parses a rates response; keeps only `<CUR>/BYN` pairs. An empty or
/// malformed response is an error so callers fall back.
pub fn parse(body: &str) -> Result<Snapshot, Error> {
    let r: Resp = serde_json::from_str(body)?;
    let quotes: Vec<(String, Quote)> = r
        .rates
        .iter()
        .filter(|x| x.sell_currency_name == "BYN" && x.buy_currency_name != "BYN")
        .filter_map(|x| {
            let buy = parse_scaled(&x.scaled_buy_amount, x.buy_currency_scale)?;
            let sell = parse_scaled(&x.scaled_sell_amount, x.buy_currency_scale)?;
            (buy > 0 && sell > 0).then(|| (x.buy_currency_name.to_uppercase(), Quote { buy, sell }))
        })
        .collect();
    if quotes.is_empty() {
        return Err(Error::Invalid("bsb: no BYN quotes".into()));
    }
    Ok(Snapshot {
        from_ms: r.from_time,
        next_ms: r.next_time,
        quotes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"fromTime":1795289400000,"nextTime":null,"previousTime":1795288800000,"rates":[
      {"buyAmount":2.982,"sellAmount":3.0,"scaledBuyAmount":"2.9820","scaledSellAmount":"3.0000","buyAmountDelta":"0","sellAmountDelta":"0","buyCurrencyName":"USD","sellCurrencyName":"BYN","buyCurrencyDescription":"","sellCurrencyDescription":"","buyCurrencyScale":1},
      {"buyAmount":0.034753,"sellAmount":0.03644,"scaledBuyAmount":"3.4753","scaledSellAmount":"3.6440","buyAmountDelta":"0","sellAmountDelta":"0","buyCurrencyName":"RUB","sellCurrencyName":"BYN","buyCurrencyDescription":"","sellCurrencyDescription":"","buyCurrencyScale":100},
      {"buyAmount":82.25,"sellAmount":86.0363,"scaledBuyAmount":"82.2500","scaledSellAmount":"86.0363","buyAmountDelta":"0","sellAmountDelta":"0","buyCurrencyName":"USD","sellCurrencyName":"RUB","buyCurrencyDescription":"","sellCurrencyDescription":"","buyCurrencyScale":1}
    ]}"#;

    #[test]
    fn parse_scaled_cases() {
        assert_eq!(parse_scaled("2.9905", 1), Some(2_990_500));
        assert_eq!(parse_scaled("3.4753", 100), Some(34_753));
        assert_eq!(parse_scaled("0.8080", 10), Some(80_800));
        assert_eq!(parse_scaled("3", 1), Some(3_000_000));
        assert_eq!(parse_scaled("abc", 1), None);
        assert_eq!(parse_scaled("-1.0", 1), None);
        assert_eq!(parse_scaled("1.0", 0), None);
    }

    #[test]
    fn parse_real_response() {
        let s = parse(SAMPLE).unwrap();
        assert_eq!(s.from_ms, 1_795_289_400_000);
        assert_eq!(s.next_ms, None);
        assert_eq!(
            s.quotes,
            vec![
                ("USD".to_string(), Quote { buy: 2_982_000, sell: 3_000_000 }),
                ("RUB".to_string(), Quote { buy: 34_753, sell: 36_440 }),
            ]
        );
    }

    #[test]
    fn parse_garbage_is_error() {
        assert!(parse("<html>").is_err());
        assert!(parse(r#"{"fromTime":1,"nextTime":null,"rates":[]}"#).is_err());
    }

    #[test]
    fn request_body_shape() {
        let b = request_body(RateKind::BsbCard, 42);
        assert_eq!(b["type"], "CARD");
        assert_eq!(b["period"], 42);
        assert_eq!(api_type(RateKind::Nbrb), None);
    }
}
