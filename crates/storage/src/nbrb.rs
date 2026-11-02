//! National Bank of Belarus public rates API (api.nbrb.by), free and no key needed.

use serde::Deserialize;

/// GET https://api.nbrb.by/exrates/rates/{CUR}?parammode=2
#[derive(Debug, Deserialize)]
pub struct NbrbRate {
    #[serde(rename = "Cur_Abbreviation")]
    pub abbr: String,
    #[serde(rename = "Cur_OfficialRate")]
    pub official_rate: f64,
    #[serde(rename = "Cur_Scale")]
    pub scale: i64,
    #[serde(rename = "Date")]
    pub date: String,
}

/// Per-unit BYN rate scaled to 1e6 (integer math elsewhere).
/// e.g. USD 3.25 → 3_250_000 ; JPY scale=10, official 21.3 per 10 → 2_130_000.
pub fn per_unit_scaled(r: &NbrbRate) -> i64 {
    ((r.official_rate / r.scale as f64) * 1_000_000.0).round() as i64
}

/// The API returns ISO datetimes; we keep only the date part.
pub fn date_of(r: &NbrbRate) -> &str {
    r.date.split('T').next().unwrap_or(&r.date)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaled_rate_per_unit() {
        let usd = NbrbRate {
            abbr: "USD".into(),
            official_rate: 3.25,
            scale: 1,
            date: "2026-11-01T00:00:00".into(),
        };
        assert_eq!(per_unit_scaled(&usd), 3_250_000);
        let jpy = NbrbRate {
            abbr: "JPY".into(),
            official_rate: 21.3,
            scale: 10,
            date: "".into(),
        };
        assert_eq!(per_unit_scaled(&jpy), 2_130_000);
        assert_eq!(date_of(&usd), "2026-11-01");
    }
}
