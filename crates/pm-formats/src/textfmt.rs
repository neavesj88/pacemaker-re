//! How text goes into a CSV cell, in one place so the guard against spreadsheet formulas cannot
//! differ between the files that carry it (the track list, the interop CSV, the archive listing).

/// True when a spreadsheet would run a cell that starts like this as a formula.
fn runs_as_formula(s: &str) -> bool {
    s.starts_with(['=', '+', '-', '@', '\t', '\r'])
}

/// `s` as text for a spreadsheet: a leading `= + - @` (or a tab / return) gets an apostrophe, so
/// the cell shows the text instead of running it.
fn guarded(s: &str) -> String {
    if runs_as_formula(s) { format!("'{s}") } else { s.to_string() }
}

/// RFC 4180 quoting: wrapped in quotes, with quotes doubled, when the text needs it.
fn quote_if_needed(s: &str) -> String {
    if s.contains(['"', ',', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// A text cell: formula-guarded, quoted only where it needs to be.
pub fn csv_text(s: &str) -> String {
    quote_if_needed(&guarded(s))
}

/// A cell that is always quoted. `text` cells are formula-guarded; numbers and hashes (`false`)
/// are left as they are, since a negative number must stay a number.
pub fn csv_quoted(s: &str, text: bool) -> String {
    let s = if text { guarded(s) } else { s.to_string() };
    format!("\"{}\"", s.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_cells_cannot_run_as_formulas_and_are_quoted_only_when_needed() {
        assert_eq!(csv_text("=SUM(1,2)"), "\"'=SUM(1,2)\"");
        assert_eq!(csv_text("@x"), "'@x");
        assert_eq!(csv_text("-1"), "'-1");
        assert_eq!(csv_text("\tx"), "'\tx");
        assert_eq!(csv_text("plain"), "plain");
        assert_eq!(csv_text("a \"b\""), "\"a \"\"b\"\"\"");
    }

    #[test]
    fn quoted_cells_are_always_quoted_and_only_text_is_guarded() {
        assert_eq!(csv_quoted("a,\"b\"", true), "\"a,\"\"b\"\"\"");
        assert_eq!(csv_quoted("=1+1", true), "\"'=1+1\"");
        assert_eq!(csv_quoted("-5", false), "\"-5\"");
        assert_eq!(csv_quoted("plain", true), "\"plain\"");
    }
}
