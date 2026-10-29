//! Independent package identity and structured OOXML facts.
use crate::Result;
use roxmltree::{Document, Node};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const S: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const DOC_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn is(node: Node<'_, '_>, namespace: &str, name: &str) -> bool {
    node.is_element()
        && node.tag_name().namespace() == Some(namespace)
        && node.tag_name().name() == name
}
fn local<'a, 'input>(node: Node<'a, 'input>) -> &'input str {
    node.tag_name().name()
}
fn child<'a, 'input>(
    node: Node<'a, 'input>,
    namespace: &str,
    name: &str,
) -> Option<Node<'a, 'input>> {
    node.children().find(|item| is(*item, namespace, name))
}
fn attr(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.attribute(name).map(str::to_owned)
}
fn attrs(node: Node<'_, '_>) -> Value {
    let mut values = Map::new();
    for attribute in node.attributes() {
        values.insert(attribute.name().to_owned(), json!(attribute.value()));
    }
    Value::Object(values)
}
fn texts(node: Node<'_, '_>, namespace: &str) -> String {
    node.descendants()
        .filter(|part| is(*part, namespace, "t"))
        .filter_map(|part| part.text())
        .collect::<Vec<_>>()
        .join("")
}
fn read_zip(archive: &mut ZipArchive<File>, name: &str) -> Result<Vec<u8>> {
    let mut file = archive.by_name(name)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}
fn read_xml(archive: &mut ZipArchive<File>, name: &str) -> Result<String> {
    Ok(String::from_utf8(read_zip(archive, name)?)?)
}
fn names(archive: &ZipArchive<File>) -> Vec<String> {
    archive.file_names().map(str::to_owned).collect()
}
fn has(archive: &ZipArchive<File>, name: &str) -> bool {
    archive.file_names().any(|part| part == name)
}
pub fn digest(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
pub fn package_kind(path: &Path) -> &'static str {
    let Ok(bytes) = fs::read(path) else {
        return "unknown";
    };
    if bytes.starts_with(b"%PDF-") {
        return "pdf";
    }
    let Ok(mut archive) = ZipArchive::new(std::io::Cursor::new(bytes)) else {
        return "unknown";
    };
    if let Ok(mut mime) = archive.by_name("mimetype") {
        let mut value = String::new();
        if mime.read_to_string(&mut value).is_err() {
            return "unknown-zip";
        }
        return match value.as_str() {
            "application/vnd.oasis.opendocument.text" => "odt",
            "application/vnd.oasis.opendocument.spreadsheet" => "ods",
            "application/vnd.oasis.opendocument.presentation" => "odp",
            "application/vnd.storyteller.deckcraft+zip" => "deck-native",
            _ => "unknown-zip",
        };
    }
    let present = |name: &str| archive.file_names().any(|part| part == name);
    if present("[Content_Types].xml") {
        for (part, kind) in [
            ("word/document.xml", "docx"),
            ("xl/workbook.xml", "xlsx"),
            ("ppt/presentation.xml", "pptx"),
        ] {
            if present(part) {
                return kind;
            }
        }
    }
    "unknown"
}
fn media(archive: &mut ZipArchive<File>, prefix: &str) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for name in names(archive) {
        if name.starts_with(prefix) && !name.ends_with('/') {
            result.push(format!("{:x}", Sha256::digest(read_zip(archive, &name)?)));
        }
    }
    result.sort();
    Ok(result)
}
pub fn writer(path: &Path) -> Result<Value> {
    let mut archive = ZipArchive::new(File::open(path)?)?;
    let xml = read_xml(&mut archive, "word/document.xml")?;
    let document = Document::parse(&xml)?;
    let paragraphs = document
        .descendants()
        .filter(|part| is(*part, W, "p"))
        .map(|paragraph| {
            let runs = paragraph
                .descendants()
                .filter(|part| is(*part, W, "r"))
                .map(|run| {
                    let mut format = Map::new();
                    if let Some(properties) = child(run, W, "rPr") {
                        for item in properties.children().filter(|item| item.is_element()) {
                            format.insert(local(item).to_owned(), attrs(item));
                        }
                    }
                    json!({"text":texts(run, W), "format":format})
                })
                .collect::<Vec<_>>();
            json!({"text":texts(paragraph, W), "runs":runs})
        })
        .collect::<Vec<_>>();
    let sections = document
        .descendants()
        .filter(|part| is(*part, W, "sectPr"))
        .map(|section| {
            let mut values = Map::new();
            for item in section.children().filter(|item| item.is_element()) {
                if ["pgSz", "pgMar", "cols", "type"].contains(&local(item)) {
                    values.insert(local(item).to_owned(), attrs(item));
                }
            }
            Value::Object(values)
        })
        .collect::<Vec<_>>();
    let tables = document
        .descendants()
        .filter(|part| is(*part, W, "tbl"))
        .map(|table| {
            table
                .children()
                .filter(|row| is(*row, W, "tr"))
                .map(|row| {
                    row.children()
                        .filter(|cell| is(*cell, W, "tc"))
                        .map(|cell| texts(cell, W))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let changes = document.descendants().filter(|part| part.is_element() && ["ins", "del"].contains(&local(*part))).map(|change| {
        json!({"kind":local(change),"author":change.attribute((W,"author")).unwrap_or(""),"text":texts(change,W)})
    }).collect::<Vec<_>>();
    let mut stories = Vec::new();
    for name in names(&archive) {
        if (name.starts_with("word/header") || name.starts_with("word/footer"))
            && name.ends_with(".xml")
        {
            let xml = read_xml(&mut archive, &name)?;
            let doc = Document::parse(&xml)?;
            stories.push(json!({"kind":if name.starts_with("word/header"){"header"}else{"footer"},"paragraphs":doc.descendants().filter(|part|is(*part,W,"p")).map(|p|texts(p,W)).collect::<Vec<_>>()}));
        }
    }
    stories.sort_by_key(|item| {
        (
            item["kind"].as_str().unwrap_or("").to_owned(),
            item["paragraphs"].to_string(),
        )
    });
    let mut comments = Vec::new();
    if has(&archive, "word/comments.xml") {
        let xml = read_xml(&mut archive, "word/comments.xml")?;
        let doc = Document::parse(&xml)?;
        comments = doc
            .descendants()
            .filter(|part| is(*part, W, "comment"))
            .map(|c| json!({"author":c.attribute((W,"author")).unwrap_or(""),"text":texts(c,W)}))
            .collect();
    }
    let mut links = Vec::new();
    if has(&archive, "word/_rels/document.xml.rels") {
        let xml = read_xml(&mut archive, "word/_rels/document.xml.rels")?;
        let doc = Document::parse(&xml)?;
        links = doc
            .root_element()
            .children()
            .filter(|part| {
                part.is_element()
                    && part
                        .attribute("Type")
                        .is_some_and(|v| v.ends_with("/hyperlink"))
            })
            .filter_map(|part| attr(part, "Target"))
            .collect();
        links.sort();
    }
    Ok(
        json!({"paragraphs":paragraphs,"tables":tables,"sections":sections,"stories":stories,"comments":comments,"changes":changes,"links":links,"media":media(&mut archive,"word/media/")?}),
    )
}
pub fn spreadsheet(path: &Path) -> Result<Value> {
    let mut archive = ZipArchive::new(File::open(path)?)?;
    let book_xml = read_xml(&mut archive, "xl/workbook.xml")?;
    let book = Document::parse(&book_xml)?;
    let shared = if has(&archive, "xl/sharedStrings.xml") {
        let xml = read_xml(&mut archive, "xl/sharedStrings.xml")?;
        let doc = Document::parse(&xml)?;
        doc.descendants()
            .filter(|part| is(*part, S, "si"))
            .map(|part| texts(part, S))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut formats = Vec::new();
    if has(&archive, "xl/styles.xml") {
        let xml = read_xml(&mut archive, "xl/styles.xml")?;
        let doc = Document::parse(&xml)?;
        let custom = doc
            .descendants()
            .filter(|part| is(*part, S, "numFmt"))
            .filter_map(|part| Some((attr(part, "numFmtId")?, attr(part, "formatCode")?)))
            .collect::<BTreeMap<_, _>>();
        if let Some(xfs) = doc.descendants().find(|part| is(*part, S, "cellXfs")) {
            formats = xfs
                .children()
                .filter(|part| is(*part, S, "xf"))
                .map(|xf| {
                    let id = xf.attribute("numFmtId").unwrap_or("0");
                    custom
                        .get(id)
                        .cloned()
                        .unwrap_or_else(|| format!("builtin:{id}"))
                })
                .collect();
        }
    }
    let rel_xml = read_xml(&mut archive, "xl/_rels/workbook.xml.rels")?;
    let rel_doc = Document::parse(&rel_xml)?;
    let rels = rel_doc
        .root_element()
        .children()
        .filter(|part| part.is_element())
        .filter_map(|part| Some((attr(part, "Id")?, attr(part, "Target")?)))
        .collect::<BTreeMap<_, _>>();
    let mut sheets = Vec::new();
    for sheet in book.descendants().filter(|part| is(*part, S, "sheet")) {
        let relation = sheet
            .attribute((DOC_R, "id"))
            .ok_or("Workbook sheet relation missing")?;
        let target = rels
            .get(relation)
            .ok_or("Workbook relation target missing")?;
        let name = if target.starts_with('/') {
            target.trim_start_matches('/').to_owned()
        } else {
            format!("xl/{target}")
        };
        let xml = read_xml(&mut archive, &name)?;
        let doc = Document::parse(&xml)?;
        let mut cells = Map::new();
        for cell in doc.descendants().filter(|part| is(*part, S, "c")) {
            let reference = cell.attribute("r").ok_or("Cell reference missing")?;
            let kind = cell.attribute("t").unwrap_or("n");
            let raw = child(cell, S, "v")
                .and_then(|node| node.text())
                .map(str::to_owned);
            let value = if kind == "s" {
                Some(
                    shared
                        .get(
                            raw.as_deref()
                                .ok_or("Shared index missing")?
                                .parse::<usize>()?,
                        )
                        .ok_or("Shared index outside table")?
                        .clone(),
                )
            } else if kind == "inlineStr" {
                Some(texts(cell, S))
            } else {
                raw
            };
            let style = cell.attribute("s").unwrap_or("0").parse::<usize>()?;
            let number_format = if formats.is_empty() {
                "builtin:0".to_owned()
            } else {
                formats
                    .get(style)
                    .ok_or("Cell style outside table")?
                    .clone()
            };
            cells.insert(reference.to_owned(),json!({"value":value,"type":kind,"formula":child(cell,S,"f").and_then(|node|node.text()),"numberFormat":number_format}));
        }
        let filter = doc
            .descendants()
            .find(|part| is(*part, S, "autoFilter"))
            .map(|node| {
                if !node.children().any(|child| child.is_element()) {
                    format!(
                        "<ns0:autoFilter xmlns:ns0=\"{S}\" ref=\"{}\" />",
                        node.attribute("ref").unwrap_or("")
                    )
                } else {
                    xml[node.range()].to_owned()
                }
            });
        let hidden_rows = doc
            .descendants()
            .filter(|part| is(*part, S, "row") && part.attribute("hidden") == Some("1"))
            .filter_map(|row| attr(row, "r"))
            .collect::<Vec<_>>();
        sheets.push(json!({"name":sheet.attribute("name"),"state":sheet.attribute("state").unwrap_or("visible"),"cells":cells,"filter":filter,"hiddenRows":hidden_rows}));
    }
    let mut charts = Vec::new();
    let mut chart_names = names(&archive)
        .into_iter()
        .filter(|name| name.starts_with("xl/charts/chart") && name.ends_with(".xml"))
        .collect::<Vec<_>>();
    chart_names.sort();
    for name in chart_names {
        let xml = read_xml(&mut archive, &name)?;
        let doc = Document::parse(&xml)?;
        charts.push(json!({"text":texts(doc.root_element(),A),"references":doc.descendants().filter(|part|part.is_element()&&local(*part)=="f").filter_map(|part|part.text()).collect::<Vec<_>>(),"types":doc.descendants().filter(|part|part.is_element()&&local(*part).ends_with("Chart")).map(local).collect::<Vec<_>>()}));
    }
    let names=book.descendants().filter(|part|is(*part,S,"definedName")).map(|part|json!({"name":part.attribute("name"),"sheet":part.attribute("localSheetId"),"value":part.text()})).collect::<Vec<_>>();
    Ok(json!({"sheets":sheets,"charts":charts,"names":names}))
}
pub fn presentation(path: &Path) -> Result<Value> {
    let mut archive = ZipArchive::new(File::open(path)?)?;
    let root_xml = read_xml(&mut archive, "ppt/presentation.xml")?;
    let root = Document::parse(&root_xml)?;
    let size = root
        .descendants()
        .find(|part| is(*part, P, "sldSz"))
        .map(attrs);
    let mut slide_names = names(&archive)
        .into_iter()
        .filter(|name| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
        .collect::<Vec<_>>();
    slide_names.sort_by_key(|name| {
        name.rsplit('/')
            .next()
            .unwrap_or("")
            .trim_start_matches("slide")
            .trim_end_matches(".xml")
            .parse::<usize>()
            .unwrap_or(0)
    });
    let mut slides = Vec::new();
    for name in slide_names {
        let xml = read_xml(&mut archive, &name)?;
        let doc = Document::parse(&xml)?;
        let relation = format!(
            "ppt/slides/_rels/{}.rels",
            name.rsplit('/').next().ok_or("Slide name")?
        );
        let mut layout = None;
        if has(&archive, &relation) {
            let relation_xml = read_xml(&mut archive, &relation)?;
            let relation_doc = Document::parse(&relation_xml)?;
            if let Some(target) = relation_doc
                .root_element()
                .children()
                .find(|part| {
                    part.is_element()
                        && part
                            .attribute("Type")
                            .is_some_and(|value| value.ends_with("/slideLayout"))
                })
                .and_then(|part| part.attribute("Target"))
            {
                let path = if target.starts_with('/') {
                    target.trim_start_matches('/').to_owned()
                } else {
                    normalise_relative("ppt/slides", target)
                };
                let layout_xml = read_xml(&mut archive, &path)?;
                let layout_doc = Document::parse(&layout_xml)?;
                let element = layout_doc.root_element();
                let content = child(element, P, "cSld");
                layout = Some(
                    json!({"type":element.attribute("type"),"name":content.and_then(|node|node.attribute("name"))}),
                );
            }
        }
        let transitions=doc.descendants().filter(|part|is(*part,P,"transition")).map(|part|json!({"attributes":attrs(part),"effects":part.children().filter(|node|node.is_element()).map(local).collect::<Vec<_>>()})).collect::<Vec<_>>();
        let geometry=doc.descendants().filter(|part|is(*part,A,"xfrm")).map(|part|json!({"attributes":attrs(part),"children":part.children().filter(|node|node.is_element()).map(|node|json!({"kind":local(node),"attributes":attrs(node)})).collect::<Vec<_>>()})).collect::<Vec<_>>();
        slides.push(json!({"text":texts(doc.root_element(),A),"layout":layout,"transitions":transitions,"geometry":geometry}));
    }
    let mut note_names = names(&archive)
        .into_iter()
        .filter(|name| name.starts_with("ppt/notesSlides/notesSlide") && name.ends_with(".xml"))
        .collect::<Vec<_>>();
    note_names.sort();
    let mut notes = Vec::new();
    for name in note_names {
        let xml = read_xml(&mut archive, &name)?;
        notes.push(texts(Document::parse(&xml)?.root_element(), A));
    }
    let mut theme_names = names(&archive)
        .into_iter()
        .filter(|name| name.starts_with("ppt/theme/theme") && name.ends_with(".xml"))
        .collect::<Vec<_>>();
    theme_names.sort();
    let mut fonts = Vec::new();
    for name in theme_names {
        let xml = read_xml(&mut archive, &name)?;
        let doc = Document::parse(&xml)?;
        for parent in doc
            .descendants()
            .filter(|part| part.is_element() && ["majorFont", "minorFont"].contains(&local(*part)))
        {
            for child in parent
                .children()
                .filter(|part| part.is_element() && local(*part) == "latin")
            {
                fonts.push(json!({"role":local(parent),"typeface":child.attribute("typeface")}));
            }
        }
    }
    Ok(
        json!({"slides":slides,"notes":notes,"size":size,"fonts":fonts,"media":media(&mut archive,"ppt/media/")?}),
    )
}
fn normalise_relative(base: &str, target: &str) -> String {
    let mut parts = Vec::new();
    for part in format!("{base}/{target}").split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            value => parts.push(value.to_owned()),
        }
    }
    parts.join("/")
}
pub fn differences(before: &Value, after: &Value) -> Vec<Value> {
    let mut values = Vec::new();
    compare(before, after, "", &mut values);
    values
}
fn compare(before: &Value, after: &Value, prefix: &str, values: &mut Vec<Value>) {
    if std::mem::discriminant(before) != std::mem::discriminant(after) {
        values.push(json!({"field":prefix,"before":before,"after":after}));
        return;
    }
    match (before, after) {
        (Value::Object(a), Value::Object(b)) => {
            let keys = a.keys().chain(b.keys()).cloned().collect::<BTreeSet<_>>();
            for key in keys {
                let field = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                compare(
                    a.get(&key).unwrap_or(&Value::Null),
                    b.get(&key).unwrap_or(&Value::Null),
                    &field,
                    values,
                );
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                values.push(json!({"field":prefix,"before":before,"after":after}));
            } else {
                for (index, (left, right)) in a.iter().zip(b).enumerate() {
                    compare(left, right, &format!("{prefix}[{index}]"), values);
                }
            }
        }
        _ => {
            if before != after {
                values.push(json!({"field":prefix,"before":before,"after":after}));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::FileOptions;
    fn archive(parts: &[(&str, &str)]) -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("sample.zip");
        let file = File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, data) in parts {
            zip.start_file(*name, FileOptions::default()).unwrap();
            zip.write_all(data.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        (root, path)
    }
    #[test]
    fn container_identity_ignores_suffix() {
        let (_root, path) = archive(&[
            ("mimetype", "application/vnd.storyteller.deckcraft+zip"),
            ("presentation.json", "{}"),
        ]);
        assert_eq!(package_kind(&path), "deck-native");
        let (_root, path) = archive(&[("mimetype", "application/vnd.oasis.opendocument.text")]);
        assert_eq!(package_kind(&path), "odt");
    }
    #[test]
    fn writer_keeps_header_footer_roles() {
        let doc = format!(
            "<w:document xmlns:w=\"{W}\"><w:body><w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Title</w:t></w:r><w:ins w:author=\"Fixture\"><w:r><w:t>New</w:t></w:r></w:ins></w:p><w:sectPr><w:pgSz w:w=\"123\"/></w:sectPr></w:body></w:document>"
        );
        let header =
            format!("<w:hdr xmlns:w=\"{W}\"><w:p><w:r><w:t>Header</w:t></w:r></w:p></w:hdr>");
        let footer = header.replace("Header", "Footer");
        let (_root, path) = archive(&[
            ("[Content_Types].xml", "<Types/>"),
            ("word/document.xml", &doc),
            ("word/header1.xml", &header),
            ("word/footer1.xml", &footer),
        ]);
        let facts = writer(&path).unwrap();
        assert_eq!(facts["paragraphs"][0]["text"], "TitleNew");
        assert_eq!(facts["changes"][0]["author"], "Fixture");
        assert_eq!(facts["sections"][0]["pgSz"]["w"], "123");
        assert_eq!(
            facts["stories"],
            json!([{"kind":"footer","paragraphs":["Footer"]},{"kind":"header","paragraphs":["Header"]}])
        );
    }
    #[test]
    fn spreadsheet_keeps_formula_error_and_format() {
        let workbook = format!(
            "<workbook xmlns=\"{S}\" xmlns:r=\"{DOC_R}\"><sheets><sheet name=\"Data\" r:id=\"r1\"/></sheets></workbook>"
        );
        let sheet = format!(
            "<worksheet xmlns=\"{S}\"><sheetData><row r=\"2\" hidden=\"1\"><c r=\"A2\" t=\"e\" s=\"1\"><f>UNKNOWN()</f><v>#NAME?</v></c></row></sheetData><autoFilter ref=\"A1:A2\"/></worksheet>"
        );
        let styles = format!(
            "<styleSheet xmlns=\"{S}\"><numFmts><numFmt numFmtId=\"164\" formatCode=\"yyyy-mm-dd\"/></numFmts><cellXfs><xf numFmtId=\"0\"/><xf numFmtId=\"164\"/></cellXfs></styleSheet>"
        );
        let (_root, path) = archive(&[
            ("[Content_Types].xml", "<Types/>"),
            ("xl/workbook.xml", &workbook),
            (
                "xl/_rels/workbook.xml.rels",
                "<Relationships><Relationship Id=\"r1\" Target=\"worksheets/sheet1.xml\"/></Relationships>",
            ),
            ("xl/worksheets/sheet1.xml", &sheet),
            ("xl/styles.xml", &styles),
        ]);
        let facts = spreadsheet(&path).unwrap();
        assert_eq!(
            facts["sheets"][0]["cells"]["A2"],
            json!({"value":"#NAME?","type":"e","formula":"UNKNOWN()","numberFormat":"yyyy-mm-dd"})
        );
        assert_eq!(facts["sheets"][0]["hiddenRows"], json!(["2"]));
        assert!(
            facts["sheets"][0]["filter"]
                .as_str()
                .unwrap()
                .contains("A1:A2")
        );
    }
    #[test]
    fn typed_nested_differences() {
        let rows = differences(
            &json!({"comments":[{"text":"Review"}],"count":1}),
            &json!({"comments":[],"count":true}),
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["field"], "comments");
        assert_eq!(rows[0]["before"], json!([{"text":"Review"}]));
        assert_eq!(rows[1]["field"], "count");
    }
}
