//! Period sums for the UI and the bot, converted to one currency with the
//! same integer math as repos::convert_minor. Group sums are converted
//! per (kind, account currency) group, then added.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;
use serde::Serialize;

use crate::error::Error;
use crate::repos;

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub key: Option<String>,
    pub income: i64,
    pub expense: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub currency: String,
    pub income: i64,
    pub expense: i64,
    pub missing: Vec<String>,
    pub by_day: Vec<Bucket>,
    pub by_category: Vec<Bucket>,
    pub by_owner: Vec<Bucket>,
    pub by_account: Vec<Bucket>,
}

#[derive(Clone, Copy)]
enum Dim {
    Total,
    Day,
    Month,
    Category,
    Owner,
    Account,
}

impl Dim {
    fn sql(self) -> &'static str {
        match self {
            Dim::Total => "''",
            Dim::Day => "substr(t.occurred_at,1,10)",
            Dim::Month => "substr(t.occurred_at,1,7)",
            Dim::Category => "t.category_id",
            Dim::Owner => "COALESCE(t.spent_by_user_id, t.created_by_user_id)",
            Dim::Account => "t.account_id",
        }
    }
}

fn grouped(
    conn: &Connection,
    from: &str,
    to: &str,
    cur: &str,
    dim: Dim,
    missing: &mut BTreeSet<String>,
) -> Result<Vec<Bucket>, Error> {
    let sql = format!(
        "SELECT t.kind, a.currency, {} AS k, SUM(t.amount_minor)
         FROM transactions t JOIN accounts a ON a.id = t.account_id
         WHERE t.deleted_at IS NULL AND t.kind != 'transfer'
           AND t.occurred_at >= ?1 || 'T00:00:00' AND t.occurred_at <= ?2 || 'T23:59:59.999'
         GROUP BY t.kind, a.currency, k",
        dim.sql()
    );
    let mut st = conn.prepare_cached(&sql)?;
    let rows: Vec<(String, String, Option<String>, i64)> = st
        .query_map(rusqlite::params![from, to], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<_, _>>()?;
    let mut acc: BTreeMap<Option<String>, Bucket> = BTreeMap::new();
    for (kind, currency, key, sum) in rows {
        let Some(v) = repos::convert_minor(conn, sum, &currency, cur)? else {
            missing.insert(currency);
            continue;
        };
        let b = acc
            .entry(key.clone())
            .or_insert_with(|| Bucket {
                key,
                ..Default::default()
            });
        if kind == "income" {
            b.income = b.income.saturating_add(v);
        } else {
            b.expense = b.expense.saturating_add(v);
        }
    }
    Ok(acc.into_values().collect())
}

fn by_size(mut v: Vec<Bucket>) -> Vec<Bucket> {
    v.sort_by_key(|b| std::cmp::Reverse(b.income.saturating_add(b.expense)));
    v
}

pub fn summary(conn: &Connection, from: &str, to: &str, currency: &str) -> Result<Summary, Error> {
    let mut missing = BTreeSet::new();
    let total = grouped(conn, from, to, currency, Dim::Total, &mut missing)?;
    let t = total.into_iter().next().unwrap_or_default();
    Ok(Summary {
        currency: currency.to_string(),
        income: t.income,
        expense: t.expense,
        by_day: grouped(conn, from, to, currency, Dim::Day, &mut missing)?,
        by_category: by_size(grouped(conn, from, to, currency, Dim::Category, &mut missing)?),
        by_owner: by_size(grouped(conn, from, to, currency, Dim::Owner, &mut missing)?),
        by_account: by_size(grouped(conn, from, to, currency, Dim::Account, &mut missing)?),
        missing: missing.into_iter().collect(),
    })
}

pub fn monthly(conn: &Connection, from: &str, to: &str, currency: &str) -> Result<Vec<Bucket>, Error> {
    let mut missing = BTreeSet::new();
    grouped(conn, from, to, currency, Dim::Month, &mut missing)
}
