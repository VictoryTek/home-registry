//! Item search and the organizer values of a whole inventory.
//!
//! A search finds items in the inventories the caller can see (the `inventory_access` view, the
//! only definition of access) by a case-insensitive substring in the item's name, description,
//! notes, legacy category or legacy location, or in the value of one of its select or text
//! organizers (the option's name, or the text). Image organizers are never searched: their
//! "value" is a file path. Items whose name matches come first, then the newest; the id breaks
//! ties so the order is the same on every call.
//!
//! The detail page filters the items it has loaded with the same rules (see `itemFilter.ts`);
//! `frontend/src/test/fixtures/item_search_cases.json` holds the cases both must agree on.
//!
//! The search term is user text: it is bound as a parameter with its `%`, `_` and `\` escaped,
//! and nothing here logs it or puts it in an error.

use tokio_postgres::types::ToSql;
use uuid::Uuid;

use super::{escape_like_pattern, item_from_row, DatabaseService};
use crate::models::{InventoryOrganizerValue, ItemSearchHit, ItemSearchResults, OrganizerMatch};

type BoxError = Box<dyn std::error::Error>;

/// The longest search term, in characters.
pub const MAX_TERM_CHARS: usize = 100;
/// Results returned when the caller does not ask for a number.
pub const DEFAULT_LIMIT: i64 = 100;
/// The most results one call returns; a larger `limit` is lowered to this.
pub const MAX_LIMIT: i64 = 500;

/// Why a search request cannot be answered. The API sends fixed text for these; nothing the
/// client sent is repeated.
#[derive(Debug, PartialEq, Eq)]
pub enum SearchRequestError {
    /// No term, or only white space
    MissingTerm,
    /// More than [`MAX_TERM_CHARS`] characters
    TermTooLong,
    /// A control character (NUL, newline, ...) inside the term
    ControlCharacters,
    /// `limit` below 1
    BadLimit,
}

/// The term to search for: trimmed, 1 to [`MAX_TERM_CHARS`] characters, no control characters.
///
/// # Errors
/// [`SearchRequestError`] for a term that is missing, too long or holds control characters.
pub fn clean_term(raw: Option<&str>) -> Result<String, SearchRequestError> {
    let term = raw.unwrap_or_default().trim();
    if term.is_empty() {
        return Err(SearchRequestError::MissingTerm);
    }
    if term.chars().count() > MAX_TERM_CHARS {
        return Err(SearchRequestError::TermTooLong);
    }
    if term.chars().any(char::is_control) {
        return Err(SearchRequestError::ControlCharacters);
    }
    Ok(term.to_string())
}

/// The number of results to return: [`DEFAULT_LIMIT`] when not given, at most [`MAX_LIMIT`].
///
/// # Errors
/// [`SearchRequestError::BadLimit`] for a limit below 1.
pub fn resolve_limit(raw: Option<i64>) -> Result<i64, SearchRequestError> {
    match raw {
        None => Ok(DEFAULT_LIMIT),
        Some(limit) if limit < 1 => Err(SearchRequestError::BadLimit),
        Some(limit) => Ok(limit.min(MAX_LIMIT)),
    }
}

/// One statement, so the hits, their organizer matches and the total come from one snapshot. The
/// `org` step collects the matching organizer values of all accessible items in one pass over
/// the value table (a per-item subquery was 60,000 index probes); the WHERE and the "why did
/// this match" list both read it. `$1` is the escaped lower-case pattern, `$2` the user, `$3`
/// the limit.
pub const SEARCH_SQL: &str = "
WITH access AS MATERIALIZED (
    SELECT DISTINCT inventory_id FROM inventory_access WHERE user_id = $2
),
org AS MATERIALIZED (
    SELECT v.item_id,
           array_agg(t.name ORDER BY t.display_order, t.id) AS organizers,
           array_agg(COALESCE(o.name, v.text_value) ORDER BY t.display_order, t.id) AS vals
    FROM item_organizer_values v
    JOIN organizer_types t ON t.id = v.organizer_type_id
    LEFT JOIN organizer_options o ON o.id = v.organizer_option_id
    WHERE t.inventory_id IN (SELECT inventory_id FROM access)
      AND ((t.input_type = 'select' AND LOWER(o.name) LIKE $1 ESCAPE '\\')
        OR (t.input_type = 'text' AND LOWER(v.text_value) LIKE $1 ESCAPE '\\'))
    GROUP BY v.item_id
)
SELECT i.id, i.inventory_id, i.name, i.description, i.category, i.location,
       i.purchase_date::text, i.purchase_price::float8, i.warranty_expiry::text, i.notes,
       i.quantity, i.created_at, i.updated_at,
       inv.name,
       m.organizers,
       m.vals,
       COUNT(*) OVER ()
FROM items i
JOIN inventories inv ON inv.id = i.inventory_id
LEFT JOIN org m ON m.item_id = i.id
WHERE i.inventory_id IN (SELECT inventory_id FROM access)
  AND (LOWER(i.name) LIKE $1 ESCAPE '\\'
    OR LOWER(i.description) LIKE $1 ESCAPE '\\'
    OR LOWER(i.notes) LIKE $1 ESCAPE '\\'
    OR LOWER(i.category) LIKE $1 ESCAPE '\\'
    OR LOWER(i.location) LIKE $1 ESCAPE '\\'
    OR m.item_id IS NOT NULL)
ORDER BY (LOWER(i.name) LIKE $1 ESCAPE '\\') DESC, i.created_at DESC, i.id DESC
LIMIT $3";

impl DatabaseService {
    /// Search the items `user_id` can see. `term` must come from [`clean_term`] and `limit` from
    /// [`resolve_limit`].
    ///
    /// # Errors
    /// Database errors.
    pub async fn search_items(
        &self,
        user_id: Uuid,
        term: &str,
        limit: i64,
    ) -> Result<ItemSearchResults, BoxError> {
        let client = self.pool.get().await?;
        let pattern = format!("%{}%", escape_like_pattern(&term.to_lowercase()));
        let params: [&(dyn ToSql + Sync); 3] = [&pattern, &user_id, &limit];
        let rows = client.query(SEARCH_SQL, &params).await?;

        let total = rows.first().map_or(0, |row| row.get::<_, i64>(16));
        let results = rows
            .iter()
            .map(|row| {
                let organizers: Option<Vec<String>> = row.get(14);
                let values: Option<Vec<String>> = row.get(15);
                let matches = organizers
                    .unwrap_or_default()
                    .into_iter()
                    .zip(values.unwrap_or_default())
                    .map(|(organizer, value)| OrganizerMatch { organizer, value })
                    .collect();
                ItemSearchHit {
                    item: item_from_row(row),
                    inventory_name: row.get(13),
                    matches,
                }
            })
            .collect();
        Ok(ItemSearchResults {
            results,
            total,
            limit,
        })
    }

    /// The select and text organizer values of every item in an inventory, one row per value, so
    /// the detail page can filter and show them without a request per item. Image organizers
    /// and values without a name or text (an option that was deleted) are left out.
    ///
    /// # Errors
    /// Database errors.
    pub async fn get_inventory_organizer_values(
        &self,
        inventory_id: i32,
    ) -> Result<Vec<InventoryOrganizerValue>, BoxError> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT v.item_id, v.organizer_type_id, v.organizer_option_id,
                        COALESCE(o.name, v.text_value)
                 FROM item_organizer_values v
                 JOIN items i ON i.id = v.item_id
                 JOIN organizer_types t ON t.id = v.organizer_type_id
                 LEFT JOIN organizer_options o ON o.id = v.organizer_option_id
                 WHERE i.inventory_id = $1
                   AND t.inventory_id = $1
                   AND ((t.input_type = 'select' AND o.name IS NOT NULL)
                     OR (t.input_type = 'text' AND v.text_value IS NOT NULL AND v.text_value <> ''))
                 ORDER BY v.item_id, t.display_order, t.id",
                &[&inventory_id],
            )
            .await?;
        Ok(rows
            .iter()
            .map(|row| InventoryOrganizerValue {
                item_id: row.get(0),
                organizer_type_id: row.get(1),
                organizer_option_id: row.get(2),
                value: row.get(3),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_term_is_trimmed_and_must_have_something_in_it() {
        assert_eq!(clean_term(Some("  drill \t")).as_deref(), Ok("drill"));
        assert_eq!(clean_term(Some("a b")).as_deref(), Ok("a b"));
        for blank in [None, Some(""), Some("   "), Some("\t\n"), Some("\u{a0}")] {
            assert_eq!(clean_term(blank), Err(SearchRequestError::MissingTerm));
        }
    }

    #[test]
    fn the_length_limit_counts_characters_after_trimming() {
        assert!(clean_term(Some(&"x".repeat(100))).is_ok());
        assert!(
            clean_term(Some(&"é".repeat(100))).is_ok(),
            "200 bytes, 100 characters"
        );
        assert!(clean_term(Some(&format!("  {}  ", "x".repeat(100)))).is_ok());
        assert_eq!(
            clean_term(Some(&"x".repeat(101))),
            Err(SearchRequestError::TermTooLong)
        );
        assert_eq!(
            clean_term(Some(&"é".repeat(101))),
            Err(SearchRequestError::TermTooLong)
        );
    }

    #[test]
    fn control_characters_inside_the_term_are_refused() {
        for text in ["a\0b", "a\nb", "a\rb", "a\tb", "\u{7f}x", "x\u{1b}"] {
            assert_eq!(
                clean_term(Some(text)),
                Err(SearchRequestError::ControlCharacters),
                "{text:?}"
            );
        }
        // Wildcard and quote characters are ordinary text
        for text in ["100%", "a_b", "\\", "it's", "\"q\"", "ünï"] {
            assert!(clean_term(Some(text)).is_ok(), "{text:?}");
        }
    }

    #[test]
    fn the_limit_defaults_and_is_capped() {
        assert_eq!(resolve_limit(None), Ok(100));
        assert_eq!(resolve_limit(Some(1)), Ok(1));
        assert_eq!(resolve_limit(Some(500)), Ok(500));
        assert_eq!(resolve_limit(Some(501)), Ok(500));
        assert_eq!(resolve_limit(Some(i64::MAX)), Ok(500));
        assert_eq!(resolve_limit(Some(0)), Err(SearchRequestError::BadLimit));
        assert_eq!(resolve_limit(Some(-5)), Err(SearchRequestError::BadLimit));
        assert_eq!(
            resolve_limit(Some(i64::MIN)),
            Err(SearchRequestError::BadLimit)
        );
    }
}
