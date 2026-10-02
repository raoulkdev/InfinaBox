//! Turns a design document, spreadsheet or notes file the person drops on a
//! Documents board into Markdown, so the AI (and search) can read it: PDFs
//! (their text), Word `.docx` (paragraphs and tables' text), spreadsheets
//! (`.xlsx`, `.xls`, `.ods`, one table per sheet) and `.csv` / `.tsv`.
//!
//! Only text comes across: pictures, layout and formulas don't (a formula
//! cell brings its last calculated value). A scanned PDF with no text layer
//! has nothing to read, and says so instead of importing an empty page.

use std::io::{Cursor, Read};

use anyhow::{Context, Result, bail};
use calamine::{Data, Reader};
use serde::Serialize;

/// Rows shown per table, and columns; the rest is counted in the note.
const MAX_ROWS: usize = 500;
const MAX_COLS: usize = 30;
const MAX_SHEETS: usize = 20;
const MAX_TEXT_BYTES: usize = 400 * 1024;
const MAX_FILE_BYTES: usize = 40 * 1024 * 1024;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Imported {
    pub title: String,
    pub markdown: String,
    /// What didn't come across (rows cut, text shortened), for the person.
    pub note: Option<String>,
}

/// File kinds this can read, by extension (lowercase, no dot).
pub fn can_import(file_name: &str) -> bool {
    matches!(
        extension(file_name).as_str(),
        "pdf" | "docx" | "xlsx" | "xlsm" | "xls" | "ods" | "csv" | "tsv"
    )
}

fn extension(file_name: &str) -> String {
    file_name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

fn title_of(file_name: &str) -> String {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let stem = base.rsplit_once('.').map(|(s, _)| s).unwrap_or(base);
    let title = stem.replace(['_', '-'], " ");
    let title = title.trim();
    if title.is_empty() { "Imported document".into() } else { title.to_string() }
}

pub fn to_markdown(file_name: &str, bytes: &[u8]) -> Result<Imported> {
    if bytes.len() > MAX_FILE_BYTES {
        bail!("That file is too big to import (over {} MB).", MAX_FILE_BYTES / 1024 / 1024);
    }
    let title = title_of(file_name);
    let mut notes = Vec::new();
    let body = match extension(file_name).as_str() {
        "pdf" => pdf_text(bytes, &mut notes)?,
        "docx" => docx_text(bytes, &mut notes)?,
        "xlsx" | "xlsm" | "xls" | "ods" => workbook(bytes, &mut notes)?,
        "csv" => table_from_text(bytes, b',', &mut notes)?,
        "tsv" => table_from_text(bytes, b'\t', &mut notes)?,
        other => bail!("InfinaBox can't read .{other} files yet (it reads PDF, Word, spreadsheets and CSV)."),
    };
    if body.trim().is_empty() {
        bail!("There was nothing to read in that file.");
    }
    Ok(Imported {
        title,
        markdown: body,
        note: (!notes.is_empty()).then(|| notes.join(" ")),
    })
}

// ---- Text ----

fn clip(mut text: String, notes: &mut Vec<String>) -> String {
    if text.len() > MAX_TEXT_BYTES {
        let mut cut = MAX_TEXT_BYTES;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        notes.push("The text was long, so only the first part came across.".into());
    }
    text
}

/// Trailing spaces off, no more than one blank line in a row.
fn tidy(text: &str) -> String {
    let mut out = String::new();
    let mut blanks = 0;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blanks += 1;
            if blanks > 1 {
                continue;
            }
        } else {
            blanks = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string() + "\n"
}

fn pdf_text(bytes: &[u8], notes: &mut Vec<String>) -> Result<String> {
    // The PDF reader panics on some malformed files; that's a failed import.
    let owned = bytes.to_vec();
    let text = std::panic::catch_unwind(move || pdf_extract::extract_text_from_mem(&owned))
        .map_err(|_| anyhow::anyhow!("That PDF couldn't be read."))?
        .map_err(|e| anyhow::anyhow!("That PDF couldn't be read ({e})."))?;
    let text = tidy(&text);
    if text.trim().is_empty() {
        bail!("That PDF has no text to read (it may be scanned pictures of pages).");
    }
    Ok(clip(text, notes))
}

fn docx_text(bytes: &[u8], notes: &mut Vec<String>) -> Result<String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).context("That isn't a Word document.")?;
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .context("That isn't a Word document.")?
        .take(MAX_FILE_BYTES as u64)
        .read_to_string(&mut xml)
        .context("That Word document couldn't be read.")?;
    // Paragraph and line ends become newlines, cells tabs; every other tag goes.
    let mut out = String::new();
    let mut rest = xml.as_str();
    while let Some(open) = rest.find('<') {
        out.push_str(&unescape(&rest[..open]));
        let Some(close) = rest[open..].find('>') else { break };
        let tag = &rest[open + 1..open + close];
        let name = tag.trim_start_matches('/').split([' ', '/']).next().unwrap_or("");
        let closing = tag.starts_with('/');
        match (name, closing) {
            ("w:p", true) | ("w:br", _) => out.push('\n'),
            ("w:tc", true) => out.push('\t'),
            ("w:tab", _) => out.push('\t'),
            _ => {}
        }
        rest = &rest[open + close + 1..];
    }
    Ok(clip(tidy(&out), notes))
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

// ---- Tables ----

fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace(['\n', '\r'], " ").trim().to_string()
}

/// A Markdown table; the first row is the header.
fn markdown_table(rows: &[Vec<String>], notes: &mut Vec<String>, what: &str) -> String {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0).min(MAX_COLS);
    if rows.is_empty() || width == 0 {
        return String::new();
    }
    if rows.iter().any(|r| r.len() > MAX_COLS) {
        notes.push(format!("{what}: only the first {MAX_COLS} columns came across."));
    }
    if rows.len() > MAX_ROWS + 1 {
        notes.push(format!("{what}: only the first {MAX_ROWS} rows of {} came across.", rows.len() - 1));
    }
    let line = |row: &Vec<String>| {
        let cells: Vec<String> = (0..width).map(|i| cell(row.get(i).map(String::as_str).unwrap_or(""))).collect();
        format!("| {} |\n", cells.join(" | "))
    };
    let mut out = line(&rows[0]);
    out.push_str(&format!("|{}\n", " --- |".repeat(width)));
    for row in rows.iter().skip(1).take(MAX_ROWS) {
        out.push_str(&line(row));
    }
    out
}

fn data_text(d: &Data) -> String {
    match d {
        Data::Empty => String::new(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", *f as i64),
        Data::Float(f) => format!("{f}"),
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn workbook(bytes: &[u8], notes: &mut Vec<String>) -> Result<String> {
    let mut book = calamine::open_workbook_auto_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|e| anyhow::anyhow!("That spreadsheet couldn't be read ({e})."))?;
    let names = book.sheet_names();
    if names.len() > MAX_SHEETS {
        notes.push(format!("Only the first {MAX_SHEETS} sheets of {} came across.", names.len()));
    }
    let mut out = String::new();
    for name in names.into_iter().take(MAX_SHEETS) {
        let Ok(range) = book.worksheet_range(&name) else { continue };
        let rows: Vec<Vec<String>> = range
            .rows()
            .map(|r| r.iter().map(data_text).collect::<Vec<_>>())
            .filter(|r| r.iter().any(|c| !c.trim().is_empty()))
            .collect();
        if rows.is_empty() {
            continue;
        }
        out.push_str(&format!("## {}\n\n", cell(&name)));
        out.push_str(&markdown_table(&rows, notes, &name));
        out.push('\n');
    }
    Ok(clip(out.trim_end().to_string() + "\n", notes))
}

/// CSV with quoted fields (`"a, b"`, doubled quotes, newlines inside quotes).
fn parse_delimited(text: &str, delimiter: u8) -> Vec<Vec<String>> {
    let delimiter = delimiter as char;
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' && field.is_empty() {
            quoted = true;
        } else if c == delimiter {
            row.push(std::mem::take(&mut field));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut field));
            if row.iter().any(|f| !f.trim().is_empty()) {
                rows.push(std::mem::take(&mut row));
            } else {
                row.clear();
            }
        } else {
            field.push(c);
        }
    }
    row.push(field);
    if row.iter().any(|f| !f.trim().is_empty()) {
        rows.push(row);
    }
    rows
}

fn table_from_text(bytes: &[u8], delimiter: u8, notes: &mut Vec<String>) -> Result<String> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_start_matches('\u{feff}');
    let rows = parse_delimited(text, delimiter);
    Ok(clip(markdown_table(&rows, notes, "The table"), notes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_come_from_file_names() {
        assert_eq!(title_of("Combat_balance-v2.xlsx"), "Combat balance v2");
        assert_eq!(title_of("C:\\docs\\Story.pdf"), "Story");
        assert_eq!(title_of(".pdf"), "Imported document");
        assert!(can_import("A.PDF") && can_import("b.csv") && !can_import("c.png"));
    }

    #[test]
    fn csv_becomes_a_table_with_quotes_and_pipes_handled() {
        let csv = "name,damage,notes\nSlime,3,\"hops, slowly\"\nBat,5,\"says \"\"hi\"\"\nand leaves\"\nOdd|name,1,x\n";
        let out = to_markdown("enemies.csv", csv.as_bytes()).unwrap();
        assert_eq!(out.title, "enemies");
        assert!(out.markdown.starts_with("| name | damage | notes |\n| --- | --- | --- |\n"), "{}", out.markdown);
        assert!(out.markdown.contains("| Slime | 3 | hops, slowly |"), "{}", out.markdown);
        assert!(out.markdown.contains("| Bat | 5 | says \"hi\" and leaves |"), "{}", out.markdown);
        assert!(out.markdown.contains("Odd\\|name"), "{}", out.markdown);
        assert_eq!(out.note, None);
    }

    #[test]
    fn long_tables_are_cut_and_the_note_says_how_much() {
        let mut csv = String::from("id,v\n");
        for i in 0..600 {
            csv.push_str(&format!("{i},x\n"));
        }
        let out = to_markdown("big.csv", csv.as_bytes()).unwrap();
        assert!(out.note.unwrap().contains("first 500 rows of 600"));
        assert_eq!(out.markdown.lines().count(), 2 + 500);
    }

    #[test]
    fn tsv_and_bom_work() {
        let out = to_markdown("t.tsv", "\u{feff}a\tb\n1\t2\n".as_bytes()).unwrap();
        assert!(out.markdown.starts_with("| a | b |"), "{}", out.markdown);
    }

    #[test]
    fn a_word_document_gives_its_paragraphs_and_cells() {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(Cursor::new(&mut buf));
            zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(
                br#"<w:document><w:body><w:p><w:r><w:t>The Hollow</w:t></w:r></w:p><w:p><w:r><w:t xml:space="preserve">A &amp; B</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>HP</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>10</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#,
            )
            .unwrap();
            zip.finish().unwrap();
        }
        let out = to_markdown("Design Doc.docx", &buf).unwrap();
        assert!(out.markdown.contains("The Hollow\nA & B\n"), "{:?}", out.markdown);
        assert!(out.markdown.contains("HP"), "{:?}", out.markdown);
        assert!(out.markdown.contains("10"), "{:?}", out.markdown);
        assert!(to_markdown("x.docx", b"not a zip").is_err());
    }

    #[test]
    fn unreadable_and_unsupported_files_say_so() {
        assert!(to_markdown("a.pdf", b"%PDF-nope").unwrap_err().to_string().contains("PDF"));
        assert!(to_markdown("a.xlsx", b"nope").unwrap_err().to_string().contains("spreadsheet"));
        assert!(to_markdown("a.png", b"x").unwrap_err().to_string().contains("can't read .png"));
        assert!(to_markdown("a.csv", b"").unwrap_err().to_string().contains("nothing to read"));
    }

    #[test]
    fn a_workbook_gives_one_table_per_sheet() {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/import/balance.xlsx")).unwrap();
        let out = to_markdown("balance.xlsx", &bytes).unwrap();
        let md = out.markdown;
        assert!(md.starts_with("## Enemies\n\n| name | hp | speed |\n| --- | --- | --- |\n"), "{md}");
        assert!(md.contains("| Slime | 10 | 1.5 |"), "{md}");
        // Whole numbers don't grow a ".0", and a pipe in a cell is escaped.
        assert!(md.contains("| Bat | 6 | 3 |"), "{md}");
        assert!(md.contains("Boss \\| big | 200 |"), "{md}");
        assert!(md.contains("## Items\n\n| item | cost |\n| --- | --- |\n| Potion | 25 |"), "{md}");
    }

    /// A one-page PDF with a proper cross-reference table.
    fn tiny_pdf(text: &str) -> Vec<u8> {
        let stream = format!("BT /F1 18 Tf 20 40 Td ({text}) Tj ET");
        let objects = [
            "<</Type/Catalog/Pages 2 0 R>>".to_string(),
            "<</Type/Pages/Kids[3 0 R]/Count 1>>".to_string(),
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 300 100]/Contents 4 0 R/Resources<</Font<</F1 5 0 R>>>>>>".to_string(),
            format!("<</Length {}>>\nstream\n{stream}\nendstream", stream.len()),
            "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".to_string(),
        ];
        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (i, body) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
        }
        let xref = pdf.len();
        pdf.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1));
        for o in offsets {
            pdf.push_str(&format!("{o:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<</Size {}/Root 1 0 R>>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        ));
        pdf.into_bytes()
    }

    #[test]
    fn a_real_pdf_gives_its_text() {
        let out = to_markdown("rules.pdf", &tiny_pdf("Boss fight rules")).unwrap();
        assert!(out.markdown.contains("Boss fight rules"), "{:?}", out.markdown);
        assert_eq!(out.title, "rules");
    }
}
