//! Inventory reports.
//!
//! The item list, the statistics, the breakdown and the CSV all describe the same items, because
//! they are built from one filter ([`build_where`]) and, for the combined report, read in one
//! `REPEATABLE READ` snapshot. Access goes through the `inventory_access` view only.
//!
//! Rules that hold in every part of a report:
//! - A missing quantity counts as 1 (the item page does the same).
//! - A missing price adds nothing to a value and is left out of the average.
//! - An item has at most one value per organizer (a database constraint), so grouping by an
//!   organizer counts every item exactly once. An item without a value, or whose option was
//!   deleted, is in the group "No value".

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::NaiveDate;
use tokio_postgres::types::ToSql;
use tokio_postgres::{GenericClient, IsolationLevel};
use uuid::Uuid;

use super::{build_order_by, escape_like_pattern, DatabaseService};
use crate::models::{
    CategoryBreakdown, InventoryReportRequest, InventoryStatistics, Item, ReportGrouping,
    ReportItem,
};

type BoxError = Box<dyn std::error::Error>;
type Params = Vec<Box<dyn ToSql + Sync>>;

/// At most this many `type:option` pairs in one `organizers` parameter.
pub const MAX_ORGANIZER_PAIRS: usize = 50;

/// The group for items that have no value for the organizer.
pub const NO_VALUE_LABEL: &str = "No value";
/// The legacy breakdown's group for items without a category.
const UNCATEGORIZED_LABEL: &str = "Uncategorized";

/// Why a report request cannot be answered. The messages the API sends for these are fixed
/// text; nothing the client sent is repeated.
#[derive(Debug, PartialEq, Eq)]
pub enum ReportRequestError {
    /// `organizers` does not have the form `type:option[,type:option...]`
    MalformedOrganizers,
    /// More than [`MAX_ORGANIZER_PAIRS`] pairs
    TooManyOrganizers,
    /// `organizers` or `group_by` without an `inventory_id`
    NeedsInventory,
    /// An organizer or option that this inventory does not have
    UnknownOrganizer,
    /// A text or image organizer where a select organizer is needed
    NotASelect,
    /// `from_date` or `to_date` is not `YYYY-MM-DD`
    InvalidDate,
}

/// A failure while resolving or running a report.
#[derive(Debug)]
pub enum ReportError {
    Request(ReportRequestError),
    Database(BoxError),
}

impl From<ReportRequestError> for ReportError {
    fn from(error: ReportRequestError) -> Self {
        ReportError::Request(error)
    }
}

impl From<tokio_postgres::Error> for ReportError {
    fn from(error: tokio_postgres::Error) -> Self {
        ReportError::Database(Box::new(error))
    }
}

impl From<deadpool_postgres::PoolError> for ReportError {
    fn from(error: deadpool_postgres::PoolError) -> Self {
        ReportError::Database(Box::new(error))
    }
}

/// The options of one organizer that an item must have (any of them), and/or "no value".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizerFilter {
    pub organizer_type_id: i32,
    /// Sorted, without duplicates
    pub option_ids: Vec<i32>,
    pub include_none: bool,
}

/// An organizer used by a report.
#[derive(Debug, Clone)]
pub struct ReportOrganizerType {
    pub id: i32,
    pub inventory_id: i32,
    pub name: String,
}

/// A report request that has been checked against the user's access and the inventory's
/// organizers.
#[derive(Debug)]
pub struct ReportScope {
    pub request: InventoryReportRequest,
    pub user_id: Uuid,
    pub filters: Vec<OrganizerFilter>,
    pub group_by: Option<ReportGrouping>,
    /// `request.from_date` and `request.to_date` as dates: a text parameter cannot be compared
    /// with a `date` column
    pub from_date: Option<NaiveDate>,
    pub to_date: Option<NaiveDate>,
}

/// Everything the JSON report holds, read from one snapshot.
#[derive(Debug)]
pub struct ReportBundle {
    pub items: Vec<ReportItem>,
    pub statistics: InventoryStatistics,
    pub breakdown: Vec<CategoryBreakdown>,
}

/// Everything the CSV export needs, read from one snapshot.
#[derive(Debug)]
pub struct CsvBundle {
    pub items: Vec<ReportItem>,
    /// Select and text organizers of the items' inventories, in display order
    pub organizer_types: Vec<ReportOrganizerType>,
}

// ---------------------------------------------------------------- parsing

/// A positive id written with digits only.
fn parse_id(text: &str) -> Option<i32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<i32>().ok().filter(|id| *id > 0)
}

/// Parse the `organizers` parameter strictly: `type:option` pairs separated by commas, where
/// both are positive integers and the option may also be the word `none`. Repeated pairs are
/// merged. Anything else is refused.
///
/// # Errors
/// [`ReportRequestError::MalformedOrganizers`] or [`ReportRequestError::TooManyOrganizers`].
pub fn parse_organizer_filters(input: &str) -> Result<Vec<OrganizerFilter>, ReportRequestError> {
    if input.split(',').count() > MAX_ORGANIZER_PAIRS {
        return Err(ReportRequestError::TooManyOrganizers);
    }

    let mut merged: BTreeMap<i32, (BTreeSet<i32>, bool)> = BTreeMap::new();
    for pair in input.split(',') {
        let (type_text, option_text) = pair
            .split_once(':')
            .ok_or(ReportRequestError::MalformedOrganizers)?;
        let type_id = parse_id(type_text).ok_or(ReportRequestError::MalformedOrganizers)?;
        let entry = merged.entry(type_id).or_default();
        if option_text == "none" {
            entry.1 = true;
        } else {
            let option_id = parse_id(option_text).ok_or(ReportRequestError::MalformedOrganizers)?;
            entry.0.insert(option_id);
        }
    }

    Ok(merged
        .into_iter()
        .map(
            |(organizer_type_id, (options, include_none))| OrganizerFilter {
                organizer_type_id,
                option_ids: options.into_iter().collect(),
                include_none,
            },
        )
        .collect())
}

// ---------------------------------------------------------------- the filter

struct Where {
    sql: String,
    params: Params,
}

/// Add a parameter and return its `$n` number.
fn bind(params: &mut Params, value: impl ToSql + Sync + 'static) -> usize {
    params.push(Box::new(value));
    params.len()
}

/// The `WHERE` clause (on `items i`) shared by the item list, the statistics and the breakdown.
fn build_where(scope: &ReportScope) -> Where {
    let request = &scope.request;
    let mut params: Params = Vec::new();
    let user = bind(&mut params, scope.user_id);
    let mut conditions = vec![format!(
        "i.inventory_id IN (SELECT inventory_id FROM inventory_access WHERE user_id = ${user})"
    )];

    if let Some(inventory_id) = request.inventory_id {
        let n = bind(&mut params, inventory_id);
        conditions.push(format!("i.inventory_id = ${n}"));
    }
    if let Some(category) = &request.category {
        let n = bind(&mut params, category.clone());
        conditions.push(format!("i.category = ${n}"));
    }
    if let Some(location) = &request.location {
        let n = bind(&mut params, format!("%{}%", escape_like_pattern(location)));
        conditions.push(format!("i.location ILIKE ${n}"));
    }
    if let Some(from) = scope.from_date {
        let n = bind(&mut params, from);
        conditions.push(format!("i.purchase_date >= ${n}"));
    }
    if let Some(to) = scope.to_date {
        let n = bind(&mut params, to);
        conditions.push(format!("i.purchase_date <= ${n}"));
    }
    if let Some(min) = request.min_price {
        let n = bind(&mut params, min);
        conditions.push(format!("i.purchase_price >= ${n}::float8"));
    }
    if let Some(max) = request.max_price {
        let n = bind(&mut params, max);
        conditions.push(format!("i.purchase_price <= ${n}::float8"));
    }

    for filter in &scope.filters {
        let type_n = bind(&mut params, filter.organizer_type_id);
        let mut alternatives = Vec::new();
        if !filter.option_ids.is_empty() {
            let options_n = bind(&mut params, filter.option_ids.clone());
            alternatives.push(format!(
                "EXISTS (SELECT 1 FROM item_organizer_values v
                         WHERE v.item_id = i.id AND v.organizer_type_id = ${type_n}
                           AND v.organizer_option_id = ANY(${options_n}))"
            ));
        }
        if filter.include_none {
            // No row, or a row whose option was deleted
            alternatives.push(format!(
                "NOT EXISTS (SELECT 1 FROM item_organizer_values v
                             WHERE v.item_id = i.id AND v.organizer_type_id = ${type_n}
                               AND v.organizer_option_id IS NOT NULL)"
            ));
        }
        conditions.push(format!("({})", alternatives.join(" OR ")));
    }

    Where {
        sql: conditions.join(" AND "),
        params,
    }
}

/// The filtered items with the columns the statistics and the breakdown need.
fn filtered_cte(where_sql: &str) -> String {
    format!(
        "filtered AS (
             SELECT i.id, i.inventory_id, i.purchase_price, i.purchase_date,
                    COALESCE(i.quantity, 1) AS qty, i.category
             FROM items i
             WHERE {where_sql}
         )"
    )
}

fn param_refs(params: &Params) -> Vec<&(dyn ToSql + Sync)> {
    params.iter().map(std::convert::AsRef::as_ref).collect()
}

// ---------------------------------------------------------------- queries

async fn items_in<C: GenericClient>(
    client: &C,
    scope: &ReportScope,
) -> Result<Vec<ReportItem>, tokio_postgres::Error> {
    let filter = build_where(scope);
    let sql = format!(
        "SELECT i.id, i.inventory_id, i.name, i.description, i.category, i.location,
                i.purchase_date::text, i.purchase_price::float8, i.warranty_expiry::text,
                i.notes, i.quantity, i.created_at, i.updated_at
         FROM items i
         WHERE {}
         ORDER BY {}, i.id",
        filter.sql,
        build_order_by(&scope.request)
    );
    let rows = client
        .query(sql.as_str(), &param_refs(&filter.params))
        .await?;

    let mut items: Vec<ReportItem> = rows
        .iter()
        .map(|row| ReportItem {
            item: Item {
                id: Some(row.get(0)),
                inventory_id: row.get(1),
                name: row.get(2),
                description: row.get(3),
                category: row.get(4),
                location: row.get(5),
                purchase_date: row.get::<_, Option<String>>(6),
                purchase_price: row.get(7),
                warranty_expiry: row.get::<_, Option<String>>(8),
                notes: row.get(9),
                quantity: row.get(10),
                created_at: row.get(11),
                updated_at: row.get(12),
            },
            organizer_values: BTreeMap::new(),
        })
        .collect();

    // Select and text values (image organizers hold file paths, not something to report on).
    // A value whose option was deleted has neither an option nor text: it is left out.
    let ids: Vec<i32> = items.iter().filter_map(|r| r.item.id).collect();
    let value_rows = client
        .query(
            "SELECT v.item_id, v.organizer_type_id, COALESCE(o.name, v.text_value)
             FROM item_organizer_values v
             JOIN organizer_types t ON t.id = v.organizer_type_id AND t.input_type <> 'image'
             LEFT JOIN organizer_options o ON o.id = v.organizer_option_id
             WHERE v.item_id = ANY($1) AND COALESCE(o.name, v.text_value) IS NOT NULL",
            &[&ids],
        )
        .await?;
    let mut values: HashMap<i32, BTreeMap<i32, String>> = HashMap::new();
    for row in &value_rows {
        values
            .entry(row.get(0))
            .or_default()
            .insert(row.get(1), row.get(2));
    }
    for report_item in &mut items {
        if let Some(found) = report_item.item.id.and_then(|id| values.remove(&id)) {
            report_item.organizer_values = found;
        }
    }
    Ok(items)
}

async fn statistics_in<C: GenericClient>(
    client: &C,
    scope: &ReportScope,
) -> Result<InventoryStatistics, tokio_postgres::Error> {
    let mut filter = build_where(scope);
    // The number of groups the breakdown shows, for the grouping in use
    let (join, groups) = match &scope.group_by {
        Some(group) => {
            let n = bind(&mut filter.params, group.organizer_type_id);
            (
                format!(
                    "LEFT JOIN item_organizer_values v
                        ON v.item_id = f.id AND v.organizer_type_id = ${n}"
                ),
                "COUNT(DISTINCT COALESCE(v.organizer_option_id, 0))::int8".to_string(),
            )
        },
        None => (
            String::new(),
            format!(
                "COUNT(DISTINCT COALESCE(NULLIF(f.category, ''), '{UNCATEGORIZED_LABEL}'))::int8"
            ),
        ),
    };
    let sql = format!(
        "WITH {}
         SELECT COUNT(*)::int8,
                COALESCE(SUM(f.purchase_price * f.qty), 0.0)::float8,
                COALESCE(SUM(f.qty), 0)::int8,
                {groups},
                COUNT(DISTINCT f.inventory_id)::int8,
                MIN(f.purchase_date)::text,
                MAX(f.purchase_date)::text,
                COALESCE(AVG(f.purchase_price), 0.0)::float8
         FROM filtered f
         {join}",
        filtered_cte(&filter.sql)
    );
    let row = client
        .query_one(sql.as_str(), &param_refs(&filter.params))
        .await?;

    Ok(InventoryStatistics {
        total_items: row.get(0),
        total_value: row.get(1),
        total_quantity: row.get(2),
        category_count: row.get(3),
        inventories_count: if scope.request.inventory_id.is_some() {
            1
        } else {
            row.get(4)
        },
        oldest_item_date: row.get(5),
        newest_item_date: row.get(6),
        average_item_value: row.get(7),
    })
}

async fn breakdown_in<C: GenericClient>(
    client: &C,
    scope: &ReportScope,
) -> Result<Vec<CategoryBreakdown>, tokio_postgres::Error> {
    let mut filter = build_where(scope);
    let (group_columns, join, group_by) = match &scope.group_by {
        Some(group) => {
            let n = bind(&mut filter.params, group.organizer_type_id);
            (
                format!("o.id, COALESCE(o.name, '{NO_VALUE_LABEL}')"),
                format!(
                    "LEFT JOIN item_organizer_values v
                        ON v.item_id = f.id AND v.organizer_type_id = ${n}
                     LEFT JOIN organizer_options o ON o.id = v.organizer_option_id"
                ),
                "o.id, o.name",
            )
        },
        None => (
            format!("NULL::int4, COALESCE(NULLIF(f.category, ''), '{UNCATEGORIZED_LABEL}')"),
            String::new(),
            "COALESCE(NULLIF(f.category, ''), 'Uncategorized')",
        ),
    };
    // Largest value first; ties by name, with "No value" after the named groups
    let sql = format!(
        "WITH {},
         totals AS (
             SELECT COALESCE(SUM(purchase_price * qty), 0.0)::float8 AS grand_total FROM filtered
         )
         SELECT {group_columns},
                COUNT(*)::int8,
                COALESCE(SUM(f.qty), 0)::int8,
                COALESCE(SUM(f.purchase_price * f.qty), 0.0)::float8 AS total_value,
                CASE WHEN t.grand_total > 0
                     THEN (COALESCE(SUM(f.purchase_price * f.qty), 0.0) / t.grand_total * 100.0)::float8
                     ELSE 0.0::float8 END
         FROM filtered f
         {join}
         CROSS JOIN totals t
         GROUP BY {group_by}, t.grand_total
         ORDER BY total_value DESC, 2 ASC",
        filtered_cte(&filter.sql)
    );
    let rows = client
        .query(sql.as_str(), &param_refs(&filter.params))
        .await?;

    let mut breakdown: Vec<CategoryBreakdown> = rows
        .iter()
        .map(|row| CategoryBreakdown {
            organizer_option_id: row.get(0),
            category: row.get(1),
            item_count: row.get(2),
            total_quantity: row.get(3),
            total_value: row.get(4),
            percentage_of_total: row.get(5),
        })
        .collect();
    // Equal values: named groups first (by name), "No value" last
    breakdown.sort_by(|a, b| {
        b.total_value
            .total_cmp(&a.total_value)
            .then_with(|| {
                let a_none = scope.group_by.is_some() && a.organizer_option_id.is_none();
                let b_none = scope.group_by.is_some() && b.organizer_option_id.is_none();
                a_none.cmp(&b_none)
            })
            .then_with(|| a.category.cmp(&b.category))
    });
    Ok(breakdown)
}

async fn organizer_types_in<C: GenericClient>(
    client: &C,
    items: &[ReportItem],
) -> Result<Vec<ReportOrganizerType>, tokio_postgres::Error> {
    let inventories: Vec<i32> = items
        .iter()
        .map(|r| r.item.inventory_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let rows = client
        .query(
            "SELECT id, inventory_id, name FROM organizer_types
             WHERE inventory_id = ANY($1) AND input_type <> 'image'
             ORDER BY display_order, id",
            &[&inventories],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|row| ReportOrganizerType {
            id: row.get(0),
            inventory_id: row.get(1),
            name: row.get(2),
        })
        .collect())
}

impl DatabaseService {
    /// Check a report request against the inventory's organizers. The caller has already
    /// checked that the user can see `inventory_id`.
    ///
    /// # Errors
    /// [`ReportError::Request`] for a request that cannot be answered, otherwise a database
    /// error.
    pub async fn resolve_report_scope(
        &self,
        request: InventoryReportRequest,
        user_id: Uuid,
    ) -> Result<ReportScope, ReportError> {
        let filters = match &request.organizers {
            Some(text) => parse_organizer_filters(text)?,
            None => Vec::new(),
        };
        let parse_date = |text: &Option<String>| -> Result<Option<NaiveDate>, ReportRequestError> {
            text.as_deref()
                .map(|t| {
                    NaiveDate::parse_from_str(t, "%Y-%m-%d")
                        .map_err(|_| ReportRequestError::InvalidDate)
                })
                .transpose()
        };
        let from_date = parse_date(&request.from_date)?;
        let to_date = parse_date(&request.to_date)?;
        if (!filters.is_empty() || request.group_by.is_some()) && request.inventory_id.is_none() {
            return Err(ReportRequestError::NeedsInventory.into());
        }
        let scope = |request, filters, group_by| ReportScope {
            request,
            user_id,
            filters,
            group_by,
            from_date,
            to_date,
        };
        let inventory_id = match request.inventory_id {
            Some(id) if !filters.is_empty() || request.group_by.is_some() => id,
            _ => return Ok(scope(request, filters, None)),
        };

        // The organizers and options of this inventory, and nothing from any other
        let client = self.pool.get().await?;
        let types: HashMap<i32, (String, String)> = client
            .query(
                "SELECT id, name, input_type FROM organizer_types WHERE inventory_id = $1",
                &[&inventory_id],
            )
            .await?
            .iter()
            .map(|row| (row.get(0), (row.get(1), row.get(2))))
            .collect();
        let option_owner: HashMap<i32, i32> = client
            .query(
                "SELECT o.id, o.organizer_type_id
                 FROM organizer_options o
                 JOIN organizer_types t ON t.id = o.organizer_type_id
                 WHERE t.inventory_id = $1",
                &[&inventory_id],
            )
            .await?
            .iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect();

        let select_type = |id: i32| -> Result<&String, ReportRequestError> {
            match types.get(&id) {
                None => Err(ReportRequestError::UnknownOrganizer),
                Some((_, kind)) if kind != "select" => Err(ReportRequestError::NotASelect),
                Some((name, _)) => Ok(name),
            }
        };

        for filter in &filters {
            select_type(filter.organizer_type_id)?;
            for option in &filter.option_ids {
                if option_owner.get(option) != Some(&filter.organizer_type_id) {
                    return Err(ReportRequestError::UnknownOrganizer.into());
                }
            }
        }
        let group_by = match request.group_by {
            Some(id) => Some(ReportGrouping {
                organizer_type_id: id,
                name: select_type(id)?.clone(),
            }),
            None => None,
        };

        Ok(scope(request, filters, group_by))
    }

    /// The items of a report.
    pub async fn report_items(&self, scope: &ReportScope) -> Result<Vec<ReportItem>, BoxError> {
        let client = self.pool.get().await?;
        Ok(items_in(&**client, scope).await?)
    }

    /// The statistics of a report.
    pub async fn report_statistics(
        &self,
        scope: &ReportScope,
    ) -> Result<InventoryStatistics, BoxError> {
        let client = self.pool.get().await?;
        Ok(statistics_in(&**client, scope).await?)
    }

    /// The breakdown of a report, by the organizer in `group_by` or else by legacy category.
    pub async fn report_breakdown(
        &self,
        scope: &ReportScope,
    ) -> Result<Vec<CategoryBreakdown>, BoxError> {
        let client = self.pool.get().await?;
        Ok(breakdown_in(&**client, scope).await?)
    }

    /// Items, statistics and breakdown from one snapshot, so they cannot disagree because
    /// something changed between the queries.
    pub async fn report_bundle(&self, scope: &ReportScope) -> Result<ReportBundle, BoxError> {
        let mut client = self.pool.get().await?;
        let tx = client
            .build_transaction()
            .isolation_level(IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()
            .await?;
        let items = items_in(&*tx, scope).await?;
        let statistics = statistics_in(&*tx, scope).await?;
        let breakdown = breakdown_in(&*tx, scope).await?;
        tx.commit().await?;
        Ok(ReportBundle {
            items,
            statistics,
            breakdown,
        })
    }

    /// Items and organizer columns for the CSV export, from one snapshot.
    pub async fn report_csv_bundle(&self, scope: &ReportScope) -> Result<CsvBundle, BoxError> {
        let mut client = self.pool.get().await?;
        let tx = client
            .build_transaction()
            .isolation_level(IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()
            .await?;
        let items = items_in(&*tx, scope).await?;
        let organizer_types = organizer_types_in(&*tx, &items).await?;
        tx.commit().await?;
        Ok(CsvBundle {
            items,
            organizer_types,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(type_id: i32, options: &[i32], none: bool) -> OrganizerFilter {
        OrganizerFilter {
            organizer_type_id: type_id,
            option_ids: options.to_vec(),
            include_none: none,
        }
    }

    #[test]
    fn pairs_are_merged_sorted_and_deduplicated() {
        assert_eq!(
            parse_organizer_filters("4:9,3:12,3:13,3:12,4:none").unwrap(),
            vec![filter(3, &[12, 13], false), filter(4, &[9], true)]
        );
        assert_eq!(
            parse_organizer_filters("7:none,7:none").unwrap(),
            vec![filter(7, &[], true)]
        );
        assert_eq!(
            parse_organizer_filters("2147483647:2147483647").unwrap(),
            vec![filter(i32::MAX, &[i32::MAX], false)]
        );
    }

    #[test]
    fn anything_but_digits_and_none_is_malformed() {
        for input in [
            "",
            ",",
            "3",
            "3:",
            ":3",
            "a:b",
            "3:a",
            "3:12:5",
            "3:12,",
            ",3:12",
            "3:12,,4:5",
            "3:-1",
            "-3:1",
            "+3:1",
            "3:+1",
            "3: 1",
            " 3:1",
            "3:1 ",
            "3:0",
            "0:3",
            "3:NONE",
            "3:None",
            "3:none ",
            "3:1.5",
            "3:1e3",
            "99999999999:1",
            "3:99999999999",
            "٣:١",
            "3:１",
            "3:12\n",
            "3:%31",
        ] {
            assert_eq!(
                parse_organizer_filters(input),
                Err(ReportRequestError::MalformedOrganizers),
                "{input:?}"
            );
        }
    }

    #[test]
    fn the_number_of_pairs_is_capped() {
        let pairs = |n: usize| {
            (1..=n)
                .map(|i| format!("1:{i}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        assert!(parse_organizer_filters(&pairs(MAX_ORGANIZER_PAIRS)).is_ok());
        assert_eq!(
            parse_organizer_filters(&pairs(MAX_ORGANIZER_PAIRS + 1)),
            Err(ReportRequestError::TooManyOrganizers)
        );
        // Counted before anything else is looked at
        assert_eq!(
            parse_organizer_filters(&",".repeat(MAX_ORGANIZER_PAIRS + 5)),
            Err(ReportRequestError::TooManyOrganizers)
        );
    }
}
