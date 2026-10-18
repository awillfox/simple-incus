//! Dense, sortable, filterable table.
//!
//! Sorting: click a sortable header (a `<button>`, so it is keyboard
//! reachable) to sort ascending; click again to toggle descending.
//! Filtering: a built-in search box above the table keeps the rows whose
//! search text contains the query (case-insensitive). The search text is
//! `search(&row)` when given, otherwise `row_key(&row)`.
//!
//! Rows are keyed by `row_key`: when `rows` changes only the rows whose value
//! changed (`T: PartialEq`) update, so focus, hover and clicks on the other
//! rows survive live refreshes. A column may narrow this further with
//! [`Column::deps`]: its cell then re-renders only when that fingerprint
//! changes (keeps e.g. a focused action button alive while other cells tick).
use std::cmp::Ordering;

use leptos::prelude::*;

use super::icons::{Icon, IconName};
use super::page::Skeleton;

/// One table column. Build with [`Column::new`] and the builder methods, or
/// as a struct literal.
pub struct Column<T> {
    pub title: &'static str,
    pub sort: Option<fn(&T, &T) -> Ordering>,
    pub render: fn(&T) -> AnyView,
    /// Right-align and use tabular numerals.
    pub numeric: bool,
    /// Optional fingerprint of the row values this cell depends on.
    pub deps: Option<fn(&T) -> String>,
}

impl<T> Clone for Column<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Column<T> {}

impl<T> Column<T> {
    pub fn new(title: &'static str, render: fn(&T) -> AnyView) -> Self {
        Column { title, sort: None, render, numeric: false, deps: None }
    }
    pub fn sortable(mut self, cmp: fn(&T, &T) -> Ordering) -> Self {
        self.sort = Some(cmp);
        self
    }
    /// Re-render this cell only when `deps(row)` changes.
    pub fn deps(mut self, deps: fn(&T) -> String) -> Self {
        self.deps = Some(deps);
        self
    }
    pub fn numeric(mut self) -> Self {
        self.numeric = true;
        self
    }
}

/// Sort state: column index and ascending flag.
pub type SortState = Option<(usize, bool)>;

/// Next sort state after clicking column `col`.
pub fn next_sort(current: SortState, col: usize) -> SortState {
    match current {
        Some((c, asc)) if c == col => Some((col, !asc)),
        _ => Some((col, true)),
    }
}

/// Filters by `text_of` (case-insensitive substring) and sorts. Pure; used by the component.
pub fn visible_rows<T: Clone>(
    rows: &[T],
    columns: &[Column<T>],
    sort: SortState,
    query: &str,
    text_of: impl Fn(&T) -> String,
) -> Vec<T> {
    let q = query.trim().to_lowercase();
    let mut out: Vec<T> = rows
        .iter()
        .filter(|r| q.is_empty() || text_of(r).to_lowercase().contains(&q))
        .cloned()
        .collect();
    if let Some((col, asc)) = sort {
        if let Some(cmp) = columns.get(col).and_then(|c| c.sort) {
            out.sort_by(|a, b| if asc { cmp(a, b) } else { cmp(b, a) });
        }
    }
    out
}

#[component]
pub fn DataTable<T>(
    #[prop(into)] rows: Signal<Vec<T>>,
    columns: Vec<Column<T>>,
    row_key: fn(&T) -> String,
    #[prop(optional)] on_row_click: Option<Callback<T>>,
    /// Text the search box matches against; defaults to `row_key`.
    #[prop(optional)]
    search: Option<fn(&T) -> String>,
    /// Search box placeholder.
    #[prop(optional, into)]
    placeholder: Option<String>,
    /// Message shown when there are no rows at all.
    #[prop(optional, into)]
    empty: Option<String>,
    /// Shows skeleton rows while true and there is no data yet.
    #[prop(optional, into)]
    loading: Signal<bool>,
    /// Extra controls rendered on the right of the toolbar.
    #[prop(optional)]
    toolbar: Option<ChildrenFn>,
) -> impl IntoView
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    let sort = RwSignal::new(None::<(usize, bool)>);
    let query = RwSignal::new(String::new());
    let columns = StoredValue::new(columns);
    let ncols = columns.with_value(|c| c.len());
    let text_of = search.unwrap_or(row_key);
    let empty = empty.unwrap_or_else(|| "Nothing here yet.".into());
    let placeholder = placeholder.unwrap_or_else(|| "Filter…".into());

    let visible = move || {
        rows.with(|r| columns.with_value(|c| visible_rows(r, c, sort.get(), &query.get(), text_of)))
    };
    let total = move || rows.with(|r| r.len());

    let header = columns.with_value(|cols| {
        cols.iter()
            .enumerate()
            .map(|(i, c)| {
                let title = c.title;
                let align = if c.numeric { "justify-end text-right" } else { "justify-start text-left" };
                let aria_sort = move || match sort.get() {
                    Some((col, true)) if col == i => Some("ascending"),
                    Some((col, false)) if col == i => Some("descending"),
                    _ => None,
                };
                let inner = if c.sort.is_some() {
                    let indicator = move || match sort.get() {
                        Some((col, true)) if col == i => view! { <Icon name=IconName::ChevronUp class="size-3.5 text-text"/> }.into_any(),
                        Some((col, false)) if col == i => view! { <Icon name=IconName::ChevronDown class="size-3.5 text-text"/> }.into_any(),
                        _ => view! { <Icon name=IconName::ChevronsUpDown class="size-3 opacity-0 transition-opacity group-hover:opacity-60"/> }.into_any(),
                    };
                    view! {
                        <button
                            type="button"
                            class=format!("group -mx-1 inline-flex h-6 items-center gap-1 rounded px-1 hover:text-text {align}")
                            on:click=move |_| sort.update(|s| *s = next_sort(*s, i))
                        >
                            {title}
                            {indicator}
                        </button>
                    }
                    .into_any()
                } else {
                    view! { <span>{title}</span> }.into_any()
                };
                view! {
                    <th
                        scope="col"
                        aria-sort=aria_sort
                        class=format!(
                            "h-9 whitespace-nowrap border-b border-border bg-surface-2 px-3 text-[12px] font-medium text-muted first:pl-4 last:pr-4 {}",
                            if c.numeric { "text-right" } else { "text-left" },
                        )
                    >
                        {inner}
                    </th>
                }
            })
            .collect_view()
    });

    let is_empty = Memo::new(move |_| visible().is_empty());
    let body = move || {
        if is_empty.get() && loading.get() {
            return (0..5)
                .map(|n| {
                    let w = ["w-32", "w-20", "w-24", "w-16", "w-28"];
                    view! {
                        <tr class="h-9 border-b border-border last:border-0">
                            {(0..ncols)
                                .map(|c| view! {
                                    <td class="px-3 first:pl-4 last:pr-4">
                                        <Skeleton class=format!("h-2.5 {}", w[(n + c) % w.len()])/>
                                    </td>
                                })
                                .collect_view()}
                        </tr>
                    }
                })
                .collect_view()
                .into_any();
        }
        if is_empty.get() {
            let msg = if total() == 0 { empty.clone() } else { "No rows match the filter.".to_string() };
            return view! {
                <tr>
                    <td colspan=ncols.to_string() class="px-4 py-10 text-center text-[13px] text-muted">{msg}</td>
                </tr>
            }
            .into_any();
        }
        let clickable = on_row_click.is_some();
        view! {
            <For
                each=visible
                key=move |row: &T| row_key(row)
                children=move |first: T| {
                    let key = row_key(&first);
                    // Latest value of this row; only notifies when it actually changed.
                    let row = Memo::new(move |prev: Option<&T>| {
                        rows.with(|r| r.iter().find(|x| row_key(x) == key).cloned())
                            .or_else(|| prev.cloned())
                            .unwrap_or_else(|| first.clone())
                    });
                    let cells = columns.with_value(|cols| {
                        cols.iter()
                            .map(|c| {
                                let cls = if c.numeric { "num text-right" } else { "text-left" };
                                let render = c.render;
                                let inner = match c.deps {
                                    Some(deps) => {
                                        let fp = Memo::new(move |_| row.with(|r| deps(r)));
                                        view! { {move || { fp.track(); row.with_untracked(|r| render(r)) }} }.into_any()
                                    }
                                    None => view! { {move || row.with(|r| render(r))} }.into_any(),
                                };
                                view! {
                                    <td class=format!("whitespace-nowrap px-3 first:pl-4 last:pr-4 {cls}")>{inner}</td>
                                }
                            })
                            .collect_view()
                    });
                    view! {
                        <tr
                            data-key=row_key(&row.get_untracked())
                            tabindex=clickable.then_some("0")
                            class=if clickable {
                                "h-9 cursor-pointer border-b border-border text-[13px] text-text transition-colors last:border-0 hover:bg-hover focus-visible:bg-hover focus-visible:outline-none"
                            } else {
                                "h-9 border-b border-border text-[13px] text-text transition-colors last:border-0 hover:bg-hover/60"
                            }
                            on:click=move |_| {
                                if let Some(cb) = on_row_click {
                                    cb.run(row.get_untracked());
                                }
                            }
                            on:keydown=move |e| {
                                if let Some(cb) = on_row_click {
                                    if e.key() == "Enter" {
                                        cb.run(row.get_untracked());
                                    }
                                }
                            }
                        >
                            {cells}
                        </tr>
                    }
                }
            />
        }
        .into_any()
    };

    let count = move || {
        let shown = visible().len();
        let all = total();
        if shown == all {
            format!("{all} {}", if all == 1 { "item" } else { "items" })
        } else {
            format!("{shown} of {all}")
        }
    };

    view! {
        <div class="panel overflow-hidden">
            <div class="flex flex-wrap items-center gap-2 border-b border-border px-3 py-2">
                <label class="relative min-w-0 flex-1 sm:max-w-[280px]">
                    <span class="sr-only">"Filter rows"</span>
                    <span class="pointer-events-none absolute inset-y-0 left-2.5 flex items-center text-muted">
                        <Icon name=IconName::Search class="size-3.5"/>
                    </span>
                    <input
                        type="search"
                        class="input h-8 w-full pl-8"
                        placeholder=placeholder
                        prop:value=move || query.get()
                        on:input=move |e| query.set(event_target_value(&e))
                    />
                </label>
                <span class="num ml-auto text-[12px] text-muted">{count}</span>
                {toolbar.map(|t| view! { <div class="flex items-center gap-2">{t()}</div> })}
            </div>
            <div class="overflow-x-auto">
                <table class="w-full border-collapse">
                    <thead>
                        <tr>{header}</tr>
                    </thead>
                    <tbody>{body}</tbody>
                </table>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Row = (String, u32);

    fn cols() -> Vec<Column<Row>> {
        vec![
            Column::new("Name", |_: &Row| ().into_any()).sortable(|a, b| a.0.cmp(&b.0)),
            Column::new("N", |_: &Row| ().into_any()).sortable(|a, b| a.1.cmp(&b.1)).numeric(),
        ]
    }

    fn rows() -> Vec<(String, u32)> {
        vec![("web".into(), 3), ("db".into(), 1), ("Worker".into(), 2)]
    }

    #[test]
    fn sort_toggles() {
        assert_eq!(next_sort(None, 1), Some((1, true)));
        assert_eq!(next_sort(Some((1, true)), 1), Some((1, false)));
        assert_eq!(next_sort(Some((1, false)), 1), Some((1, true)));
        assert_eq!(next_sort(Some((1, false)), 0), Some((0, true)));
    }

    #[test]
    fn filters_and_sorts() {
        let c = cols();
        let names = |v: Vec<(String, u32)>| v.into_iter().map(|r| r.0).collect::<Vec<_>>();
        assert_eq!(names(visible_rows(&rows(), &c, Some((1, false)), "", |r| r.0.clone())), ["web", "Worker", "db"]);
        assert_eq!(names(visible_rows(&rows(), &c, Some((0, true)), "w", |r| r.0.clone())), ["Worker", "web"]);
        assert_eq!(names(visible_rows(&rows(), &c, None, " WEB ", |r| r.0.clone())), ["web"]);
    }
}
