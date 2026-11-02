//! Excel report: "Операции" sheet (all transactions) + "Итоги" (totals by category/account).

use rusqlite::Connection;
use rust_xlsxwriter::{Format, Workbook};
use std::path::Path;

use crate::error::Error;
use crate::repos::{transaction_list, TransactionFilter};

/// Writes `<dir>/Финансы_<from>_<to>.xlsx`, returns the path.
pub fn export_xlsx(conn: &Connection, dir: &Path, from: &str, to: &str) -> Result<String, Error> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("Финансы_{from}_{to}.xlsx"));

    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    let money = Format::new().set_num_format("# ##0.00");
    let date_f = Format::new();

    // ---- Sheet 1: transactions ----
    let ws = wb.add_worksheet().set_name("Операции").map_err(xerr)?;
    let headers = [
        "Дата",
        "Тип",
        "Счёт",
        "В счёт",
        "Категория",
        "Сумма",
        "Валюта",
        "Зачислено",
        "Валюта",
        "Кто",
        "Комментарий",
    ];
    for (i, h) in headers.iter().enumerate() {
        ws.write_string_with_format(0, i as u16, *h, &bold)
            .map_err(xerr)?;
    }
    let rows = transaction_list(
        conn,
        &TransactionFilter {
            from: Some(from.into()),
            to: Some(to.into()),
            limit: Some(50000),
            ..Default::default()
        },
    )?;
    let kind_name = |k: &str| match k {
        "expense" => "Расход",
        "income" => "Доход",
        _ => "Перевод",
    };
    for (r, x) in rows.iter().enumerate() {
        let r = (r + 1) as u32;
        let s = |k: &str| x[k].as_str().unwrap_or("").to_string();
        ws.write_string_with_format(r, 0, &s("occurredAt")[..10], &date_f)
            .map_err(xerr)?;
        ws.write_string(r, 1, kind_name(&s("kind"))).map_err(xerr)?;
        ws.write_string(r, 2, s("aName")).map_err(xerr)?;
        ws.write_string(r, 3, s("a2Name")).map_err(xerr)?;
        ws.write_string(r, 4, s("cName")).map_err(xerr)?;
        ws.write_number_with_format(
            r,
            5,
            x["amountMinor"].as_f64().unwrap_or(0.0) / 100.0,
            &money,
        )
        .map_err(xerr)?;
        ws.write_string(r, 6, s("aCurrency")).map_err(xerr)?;
        if let Some(v) = x["toAmountMinor"].as_i64() {
            ws.write_number_with_format(r, 7, v as f64 / 100.0, &money)
                .map_err(xerr)?;
            ws.write_string(r, 8, s("a2Currency")).map_err(xerr)?;
        }
        ws.write_string(r, 9, s("uName")).map_err(xerr)?;
        ws.write_string(r, 10, s("comment")).map_err(xerr)?;
    }
    ws.set_column_width(0, 12).map_err(xerr)?;
    for i in 2..=4 {
        ws.set_column_width(i, 20).map_err(xerr)?;
    }
    ws.set_column_width(10, 30).map_err(xerr)?;

    // ---- Sheet 2: totals by category ----
    let ws2 = wb.add_worksheet().set_name("Итоги").map_err(xerr)?;
    for (i, h) in ["Тип", "Категория", "Валюта", "Сумма"].iter().enumerate()
    {
        ws2.write_string_with_format(0, i as u16, *h, &bold)
            .map_err(xerr)?;
    }
    let mut stmt = conn.prepare(
        "SELECT t.kind, COALESCE(c.name, ''), a.currency, SUM(t.amount_minor)
         FROM transactions t
         JOIN accounts a ON a.id = t.account_id
         LEFT JOIN categories c ON c.id = t.category_id
         WHERE t.deleted_at IS NULL AND t.kind != 'transfer'
           AND t.occurred_at >= ?1 || 'T00:00:00' AND t.occurred_at <= ?2 || 'T23:59:59.999'
         GROUP BY t.kind, c.name, a.currency
         ORDER BY t.kind DESC, SUM(t.amount_minor) DESC",
    )?;
    let mut r = 1u32;
    let mut rows = stmt.query(rusqlite::params![from, to])?;
    while let Some(row) = rows.next()? {
        let kind: String = row.get(0)?;
        let cat: String = row.get(1)?;
        let cur: String = row.get(2)?;
        let sum: i64 = row.get(3)?;
        ws2.write_string(r, 0, kind_name(&kind)).map_err(xerr)?;
        ws2.write_string(r, 1, cat).map_err(xerr)?;
        ws2.write_string(r, 2, cur).map_err(xerr)?;
        ws2.write_number_with_format(r, 3, sum as f64 / 100.0, &money)
            .map_err(xerr)?;
        r += 1;
    }
    ws2.set_column_width(0, 20).map_err(xerr)?;
    ws2.set_column_width(1, 20).map_err(xerr)?;

    wb.save(&path).map_err(xerr)?;
    Ok(path.to_string_lossy().into_owned())
}

fn xerr(e: rust_xlsxwriter::XlsxError) -> Error {
    Error::Other(format!("xlsx: {e}"))
}
