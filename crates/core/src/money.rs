//! Money is stored as integer minor units (kopecks/cents) plus an ISO currency code.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Currency(String);

impl Currency {
    pub const BASE: &'static str = "BYN";

    pub fn new(code: &str) -> Result<Self, MoneyError> {
        let code = code.trim().to_ascii_uppercase();
        if code.len() == 3 && code.chars().all(|c| c.is_ascii_alphabetic()) {
            Ok(Self(code))
        } else {
            Err(MoneyError::InvalidCurrency(code))
        }
    }

    pub fn base() -> Self {
        Self(Self::BASE.to_string())
    }

    pub fn code(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MoneyError {
    #[error("invalid currency code: {0}")]
    InvalidCurrency(String),
    #[error("invalid amount: {0}")]
    InvalidAmount(String),
    #[error("currency mismatch: {0} vs {1}")]
    CurrencyMismatch(String, String),
    #[error("amount overflow")]
    Overflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub minor: i64,
    pub currency: Currency,
}

impl Money {
    pub fn new(minor: i64, currency: Currency) -> Self {
        Self { minor, currency }
    }

    /// Parses user input like `12`, `12,5`, `1 234.56`, `-3.10` (max 2 decimals).
    pub fn parse_minor(input: &str) -> Result<i64, MoneyError> {
        let err = || MoneyError::InvalidAmount(input.to_string());
        let s: String = input
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '\u{a0}')
            .map(|c| if c == ',' { '.' } else { c })
            .collect();
        let (neg, s) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s.strip_prefix('+').unwrap_or(&s)),
        };
        let (int_part, frac_part) = match s.split_once('.') {
            Some((i, f)) => (i, f),
            None => (s, ""),
        };
        if int_part.is_empty() && frac_part.is_empty()
            || frac_part.len() > 2
            || !int_part.chars().all(|c| c.is_ascii_digit())
            || !frac_part.chars().all(|c| c.is_ascii_digit())
        {
            return Err(err());
        }
        let int: i64 = if int_part.is_empty() {
            0
        } else {
            int_part.parse().map_err(|_| err())?
        };
        let frac: i64 = match frac_part.len() {
            0 => 0,
            1 => frac_part.parse::<i64>().map_err(|_| err())? * 10,
            _ => frac_part.parse().map_err(|_| err())?,
        };
        let v = int
            .checked_mul(100)
            .and_then(|v| v.checked_add(frac))
            .ok_or(MoneyError::Overflow)?;
        Ok(if neg { -v } else { v })
    }

    pub fn checked_add(&self, other: &Money) -> Result<Money, MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch(
                self.currency.code().into(),
                other.currency.code().into(),
            ));
        }
        let minor = self
            .minor
            .checked_add(other.minor)
            .ok_or(MoneyError::Overflow)?;
        Ok(Money::new(minor, self.currency.clone()))
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.minor < 0 { "-" } else { "" };
        let abs = self.minor.unsigned_abs();
        write!(
            f,
            "{sign}{}.{:02} {}",
            abs / 100,
            abs % 100,
            self.currency.code()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_amounts() {
        assert_eq!(Money::parse_minor("12").unwrap(), 1200);
        assert_eq!(Money::parse_minor("12,5").unwrap(), 1250);
        assert_eq!(Money::parse_minor("1 234.56").unwrap(), 123456);
        assert_eq!(Money::parse_minor("-3.10").unwrap(), -310);
        assert_eq!(Money::parse_minor(".5").unwrap(), 50);
        assert!(Money::parse_minor("1.234").is_err());
        assert!(Money::parse_minor("abc").is_err());
        assert!(Money::parse_minor("").is_err());
    }

    #[test]
    fn currency_validation_and_math() {
        let byn = Currency::new("byn").unwrap();
        assert_eq!(byn.code(), "BYN");
        assert!(Currency::new("BY").is_err());
        let a = Money::new(150, byn.clone());
        let b = Money::new(-200, byn);
        assert_eq!(a.checked_add(&b).unwrap().to_string(), "-0.50 BYN");
        let usd = Money::new(1, Currency::new("USD").unwrap());
        assert!(a.checked_add(&usd).is_err());
    }
}
