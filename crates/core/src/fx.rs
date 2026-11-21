//! Which bank rate a cross-currency entry uses, and the buy/sell arithmetic.
//!
//! Money flows `from` → `to`. The bank buys the currency you hand over (buy
//! rate) and sells the one you receive (sell rate): `BYN = X × buy(X)`,
//! `Y = BYN / sell(Y)`. Quotes are BYN per 1 unit, scaled by 1e6.

use serde::{Deserialize, Serialize};

const SCALE: i128 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateKind {
    /// BSB mobile/internet banking rate (`CARD_ONLINE`).
    BsbApp,
    /// BSB card-operations rate (`CARD`).
    BsbCard,
    /// National Bank official rate.
    Nbrb,
}

impl RateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RateKind::BsbApp => "bsb_app",
            RateKind::BsbCard => "bsb_card",
            RateKind::Nbrb => "nbrb",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [RateKind::BsbApp, RateKind::BsbCard, RateKind::Nbrb]
            .into_iter()
            .find(|k| k.as_str() == s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// The suggestion can be put into the amount field.
    Fill,
    /// Shown for reference only; the user types the real amount.
    Reference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Transfer,
    Expense,
    Income,
}

impl Flow {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "transfer" => Some(Flow::Transfer),
            "expense" => Some(Flow::Expense),
            "income" => Some(Flow::Income),
            _ => None,
        }
    }
}

/// BYN per 1 unit of a currency, ×1e6. For NBRB `buy == sell`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quote {
    pub buy: i64,
    pub sell: i64,
}

/// Default rate kind and mode for an entry. `all_on_bsb` — every account
/// involved is serviced by BSB.
pub fn default_rule(
    flow: Flow,
    all_on_bsb: bool,
    entered_currency: Option<&str>,
) -> (RateKind, Mode) {
    if !all_on_bsb {
        return (RateKind::Nbrb, Mode::Fill);
    }
    match flow {
        Flow::Transfer | Flow::Expense => (RateKind::BsbApp, Mode::Fill),
        Flow::Income if entered_currency.is_some_and(|c| c.eq_ignore_ascii_case("RUB")) => {
            (RateKind::BsbApp, Mode::Reference)
        }
        Flow::Income => (RateKind::BsbCard, Mode::Fill),
    }
}

fn is_byn(c: &str) -> bool {
    c.eq_ignore_ascii_case("BYN")
}

fn round_div(num: i128, den: i128) -> Option<i64> {
    if den == 0 {
        return None;
    }
    let (num, den) = if den < 0 { (-num, -den) } else { (num, den) };
    let v = if num >= 0 {
        (num + den / 2) / den
    } else {
        (num - den / 2) / den
    };
    i64::try_from(v).ok()
}

/// Converts `amount_minor` for money flowing `from` → `to`. `known_is_from`
/// says which side the amount is in; the result is the other side. A quote is
/// required (and must be positive) for every non-BYN currency.
pub fn convert(
    amount_minor: i64,
    from: &str,
    to: &str,
    known_is_from: bool,
    q_from: Option<Quote>,
    q_to: Option<Quote>,
) -> Option<i64> {
    if from.eq_ignore_ascii_case(to) {
        return Some(amount_minor);
    }
    let valid = |q: Option<Quote>| q.filter(|q| q.buy > 0 && q.sell > 0);
    let qf = if is_byn(from) {
        None
    } else {
        Some(valid(q_from)?)
    };
    let qt = if is_byn(to) { None } else { Some(valid(q_to)?) };
    let (mut num, mut den) = (amount_minor as i128, 1i128);
    if known_is_from {
        if let Some(q) = qf {
            num *= q.buy as i128;
            den *= SCALE;
        }
        if let Some(q) = qt {
            num *= SCALE;
            den *= q.sell as i128;
        }
    } else {
        if let Some(q) = qt {
            num *= q.sell as i128;
            den *= SCALE;
        }
        if let Some(q) = qf {
            num *= SCALE;
            den *= q.buy as i128;
        }
    }
    round_div(num, den)
}

/// BYN per 1 unit of the foreign currency (×1e6) implied by two amounts.
/// None unless exactly one side is BYN and both amounts are positive.
pub fn effective_rate(from: &str, from_minor: i64, to: &str, to_minor: i64) -> Option<i64> {
    if from_minor <= 0 || to_minor <= 0 {
        return None;
    }
    let (byn, foreign) = match (is_byn(from), is_byn(to)) {
        (true, false) => (from_minor, to_minor),
        (false, true) => (to_minor, from_minor),
        _ => return None,
    };
    round_div(byn as i128 * SCALE, foreign as i128)
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn q(buy: i64, sell: i64) -> Option<Quote> {
        Some(Quote { buy, sell })
    }

    #[test]
    fn transfer_usd_to_byn_uses_buy() {
        assert_eq!(
            convert(20723, "USD", "BYN", true, q(2_990_500, 3_010_000), None),
            Some(61972)
        );
    }

    #[test]
    fn transfer_byn_to_usd_uses_sell() {
        assert_eq!(
            convert(3010, "BYN", "USD", true, None, q(2_990_500, 3_010_000)),
            Some(1000)
        );
    }

    #[test]
    fn expense_in_byn_from_usd_card_inverse_buy() {
        assert_eq!(
            convert(5000, "USD", "BYN", false, q(2_988_000, 3_010_000), None),
            Some(1673)
        );
    }

    #[test]
    fn income_byn_to_usd_card_uses_card_sell() {
        assert_eq!(
            convert(5000, "BYN", "USD", true, None, q(2_970_000, 3_050_000)),
            Some(1639)
        );
    }

    #[test]
    fn rub_per_unit_quote() {
        assert_eq!(
            convert(300000, "RUB", "BYN", true, q(34_753, 36_440), None),
            Some(10426)
        );
    }

    #[test]
    fn cross_via_byn() {
        assert_eq!(
            convert(
                1000,
                "USD",
                "EUR",
                false,
                q(2_988_000, 3_010_000),
                q(3_278_000, 3_309_000)
            ),
            Some(1107)
        );
    }

    #[test]
    fn same_currency_is_identity() {
        assert_eq!(convert(123, "usd", "USD", true, None, None), Some(123));
    }

    #[test]
    fn missing_or_zero_quote_is_none() {
        assert_eq!(convert(100, "USD", "BYN", true, None, None), None);
        assert_eq!(convert(100, "USD", "BYN", true, q(0, 0), None), None);
        assert_eq!(convert(100, "BYN", "USD", true, None, q(1, 0)), None);
    }

    #[test]
    fn rules() {
        assert_eq!(
            default_rule(Flow::Transfer, true, None),
            (RateKind::BsbApp, Mode::Fill)
        );
        assert_eq!(
            default_rule(Flow::Expense, true, Some("BYN")),
            (RateKind::BsbApp, Mode::Fill)
        );
        assert_eq!(
            default_rule(Flow::Income, true, Some("BYN")),
            (RateKind::BsbCard, Mode::Fill)
        );
        assert_eq!(
            default_rule(Flow::Income, true, Some("RUB")),
            (RateKind::BsbApp, Mode::Reference)
        );
        assert_eq!(
            default_rule(Flow::Income, false, Some("RUB")),
            (RateKind::Nbrb, Mode::Fill)
        );
        assert_eq!(
            default_rule(Flow::Expense, false, Some("BYN")),
            (RateKind::Nbrb, Mode::Fill)
        );
    }

    #[test]
    fn effective() {
        assert_eq!(effective_rate("USD", 1000, "BYN", 2921), Some(2_921_000));
        assert_eq!(effective_rate("BYN", 3010, "USD", 1000), Some(3_010_000));
        assert_eq!(effective_rate("USD", 1000, "EUR", 900), None);
        assert_eq!(effective_rate("USD", 0, "BYN", 900), None);
    }

    #[test]
    fn rate_kind_roundtrip() {
        for k in [RateKind::BsbApp, RateKind::BsbCard, RateKind::Nbrb] {
            assert_eq!(RateKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(RateKind::parse("manual"), None);
    }
}
