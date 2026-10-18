//! CSV export of an inventory report.
//!
//! Columns: ID, Inventory, Name, Description, one column per select or text organizer (by name),
//! "Legacy category" / "Legacy location" only when some exported item still holds a value in the
//! old `items.category` / `items.location` columns, then Quantity, Purchase Price, Total Value,
//! Purchase Date, Warranty Expiry, Created At.
//!
//! Text that came from a user is written so that a spreadsheet cannot read it as a formula: a
//! cell that starts with `=`, `+`, `-`, `@`, a tab, a carriage return or a line feed (or the
//! full-width forms of `=`, `+`, `-`, `@`) gets a leading `'`. Numbers and dates are not touched.
//! The quote is common practice (CWE-1236, OWASP "CSV Injection"), and spreadsheet programs that
//! save the file again can undo it, so treat an exported file as untrusted when re-importing.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use crate::db::report::ReportOrganizerType;
use crate::models::ReportItem;

/// First characters that make a spreadsheet read a cell as a formula.
const FORMULA_STARTS: [char; 11] = [
    '=', '+', '-', '@', '\t', '\r', '\n', '\u{FF1D}', '\u{FF0B}', '\u{FF0D}', '\u{FF20}',
];

/// The columns before the organizer columns.
const FRONT: [&str; 4] = ["ID", "Inventory", "Name", "Description"];
/// The columns after the organizer and legacy columns.
const BACK: [&str; 6] = [
    "Quantity",
    "Purchase Price",
    "Total Value",
    "Purchase Date",
    "Warranty Expiry",
    "Created At",
];

/// The cell text to write for user text.
#[must_use]
pub fn neutralize(text: &str) -> Cow<'_, str> {
    match text.chars().next() {
        Some(first) if FORMULA_STARTS.contains(&first) => Cow::Owned(format!("'{text}")),
        _ => Cow::Borrowed(text),
    }
}

/// A header that has not been used yet (ignoring case): a repeat gets " (2)", " (3)", ...
fn unique_header(taken: &mut HashSet<String>, wanted: &str) -> String {
    let mut candidate = wanted.to_string();
    let mut n = 2;
    while !taken.insert(candidate.to_lowercase()) {
        candidate = format!("{wanted} ({n})");
        n += 1;
    }
    candidate
}

fn holds_text(value: Option<&String>) -> bool {
    value.is_some_and(|v| !v.is_empty())
}

/// Format report items as CSV.
///
/// # Errors
/// CSV serialization errors.
pub fn format_items_as_csv(
    items: &[ReportItem],
    inventories: &HashMap<i32, String>,
    organizer_types: &[ReportOrganizerType],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Organizers with the same name in different inventories share a column
    let mut names: Vec<&str> = Vec::new();
    let mut type_of: HashMap<(i32, &str), i32> = HashMap::new();
    for organizer in organizer_types {
        if !names.contains(&organizer.name.as_str()) {
            names.push(&organizer.name);
        }
        type_of.insert((organizer.inventory_id, &organizer.name), organizer.id);
    }
    let legacy_category = items.iter().any(|r| holds_text(r.item.category.as_ref()));
    let legacy_location = items.iter().any(|r| holds_text(r.item.location.as_ref()));

    let mut taken: HashSet<String> = FRONT
        .iter()
        .chain(BACK.iter())
        .map(|h| h.to_lowercase())
        .collect();
    let mut header: Vec<String> = FRONT.iter().map(ToString::to_string).collect();
    for name in &names {
        header.push(unique_header(&mut taken, &neutralize(name)));
    }
    if legacy_category {
        header.push(unique_header(&mut taken, "Legacy category"));
    }
    if legacy_location {
        header.push(unique_header(&mut taken, "Legacy location"));
    }
    header.extend(BACK.iter().map(ToString::to_string));

    let mut writer = csv::Writer::from_writer(vec![]);
    writer.write_record(&header)?;

    for report_item in items {
        let item = &report_item.item;
        let inventory_name = inventories
            .get(&item.inventory_id)
            .map_or("Unknown", String::as_str);
        let quantity = item.quantity.unwrap_or(1);
        let total_value = item
            .purchase_price
            .map(|price| price * f64::from(quantity))
            .map(|v| format!("{v:.2}"))
            .unwrap_or_default();

        let mut record: Vec<String> = vec![
            item.id.unwrap_or(0).to_string(),
            neutralize(inventory_name).into_owned(),
            neutralize(&item.name).into_owned(),
            neutralize(item.description.as_deref().unwrap_or_default()).into_owned(),
        ];
        for name in &names {
            let cell = type_of
                .get(&(item.inventory_id, *name))
                .and_then(|type_id| report_item.organizer_values.get(type_id))
                .map_or("", String::as_str);
            record.push(neutralize(cell).into_owned());
        }
        if legacy_category {
            record.push(neutralize(item.category.as_deref().unwrap_or_default()).into_owned());
        }
        if legacy_location {
            record.push(neutralize(item.location.as_deref().unwrap_or_default()).into_owned());
        }
        record.extend([
            quantity.to_string(),
            item.purchase_price
                .map(|p| format!("{p:.2}"))
                .unwrap_or_default(),
            total_value,
            item.purchase_date.clone().unwrap_or_default(),
            item.warranty_expiry.clone().unwrap_or_default(),
            item.created_at
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default(),
        ]);
        writer.write_record(&record)?;
    }

    writer.flush()?;
    Ok(writer.into_inner()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_leading_formula_character_is_neutralised() {
        for text in [
            "=1+1",
            "+1",
            "-1",
            "@SUM(A1)",
            "\tx",
            "\rx",
            "\nx",
            "\u{FF1D}x",
            "\u{FF0B}x",
            "\u{FF0D}x",
            "\u{FF20}x",
        ] {
            assert_eq!(neutralize(text), format!("'{text}"), "{text:?}");
        }
        for text in [
            "",
            "plain",
            "a=b",
            "a-b",
            "x+y",
            "name@host",
            " =x",
            "'=x",
            "é",
            "1=1",
        ] {
            assert_eq!(neutralize(text), text, "{text:?}");
        }
    }

    #[test]
    fn repeated_headers_get_numbered_whatever_their_case() {
        let mut taken: HashSet<String> = HashSet::new();
        assert_eq!(unique_header(&mut taken, "Room"), "Room");
        assert_eq!(unique_header(&mut taken, "room"), "room (2)");
        assert_eq!(unique_header(&mut taken, "ROOM"), "ROOM (3)");
        assert_eq!(unique_header(&mut taken, "Room"), "Room (4)");
    }
}
