//! Read-only installed-provider projections for native save and reopen observations.
use crate::{Result, ensure, error};
use serde_json::{Map, Value, json};

pub trait Provider {
    fn command(&mut self, app: &str, name: &str) -> Result<Value>;
    fn tool(&mut self, name: &str, arguments: Option<Value>) -> Result<Value>;
}
impl Provider for crate::control::Control {
    fn command(&mut self, app: &str, name: &str) -> Result<Value> {
        self.command(app, name, None)
    }
    fn tool(&mut self, name: &str, arguments: Option<Value>) -> Result<Value> {
        self.tool(name, arguments)
    }
}
pub fn without_runtime_ids(value: &Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .filter(|(name, _)| name.as_str() != "id")
                .map(|(name, value)| (name.clone(), without_runtime_ids(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(without_runtime_ids).collect()),
        value => value.clone(),
    }
}
fn column(value: &str) -> Result<usize> {
    let mut number = 0usize;
    for character in value.bytes() {
        ensure(character.is_ascii_uppercase(), "Invalid sheet column")?;
        number = number * 26 + (character - b'A' + 1) as usize;
    }
    Ok(number)
}
fn extent(value: &str) -> Result<(usize, usize, String)> {
    let (first, last) = value
        .split_once(':')
        .ok_or("Expected bounded synthetic sheet extent")?;
    let parse = |text: &str| -> Result<(usize, usize)> {
        let split = text
            .find(|c: char| c.is_ascii_digit())
            .ok_or("Expected sheet row")?;
        let col = column(&text[..split])?;
        let row = text[split..].parse::<usize>()?;
        Ok((col, row))
    };
    let (left, top) = parse(first)?;
    let (right, bottom) = parse(last)?;
    ensure(
        left == 1 && top == 1 && right <= 26 && bottom <= 32 && right > 0 && bottom > 0,
        "Synthetic sheet exceeds native snapshot budget",
    )?;
    Ok((right, bottom, value.to_owned()))
}
pub fn snapshot(provider: &mut impl Provider, app: &str) -> Result<Value> {
    match app {
        "writer" => {
            let info = provider.command(app, "file.info")?;
            let document = provider.tool("inspect_document", None)?;
            ensure(
                document.is_object(),
                "Expected structured native document inspection",
            )?;
            let fields = document
                .as_object()
                .ok_or("Document object")?
                .iter()
                .filter(|(key, _)| !["selection", "canUndo", "canRedo"].contains(&key.as_str()))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<Map<_, _>>();
            let mut structure = Map::new();
            for key in [
                "pages",
                "paragraphs",
                "sections",
                "words",
                "comments",
                "trackChanges",
            ] {
                structure.insert(
                    key.to_owned(),
                    info.get(key)
                        .cloned()
                        .ok_or("Writer structure field missing")?,
                );
            }
            Ok(
                json!({"text":provider.tool("get_text",None)?,"document":without_runtime_ids(&Value::Object(fields)),"structure":structure}),
            )
        }
        "sheet" => {
            let info = provider.command(app, "document.inspect")?;
            let mut sheets = Vec::new();
            for sheet in info["sheets"].as_array().ok_or("Sheet list missing")? {
                let name = sheet["name"].as_str().ok_or("Sheet name missing")?;
                let (right, bottom, range) =
                    extent(sheet["usedRange"].as_str().ok_or("Sheet extent missing")?)?;
                let mut cells = Map::new();
                for row in 1..=bottom {
                    for col in 1..=right {
                        let address = format!("{}{}", (b'A' + col as u8 - 1) as char, row);
                        cells.insert(
                            address.clone(),
                            provider.tool(
                                "get_cell",
                                Some(json!({"cell":format!("{name}!{address}")})),
                            )?,
                        );
                    }
                }
                sheets.push(json!({"metadata":without_runtime_ids(sheet),"cells":cells,"values":provider.tool("read_range",Some(json!({"sheet":name,"range":range})))?,"formulas":provider.tool("read_range",Some(json!({"sheet":name,"range":range,"formulas":true})))?,"formatted":provider.tool("read_range",Some(json!({"sheet":name,"range":range,"formatted":true})))?}));
            }
            Ok(json!({"sheets":sheets}))
        }
        "slides" => {
            let info = provider.command(app, "document.inspect")?;
            let count = info["slides"].as_array().ok_or("Slide list missing")?.len();
            let mut slides = Vec::new();
            for index in 0..count {
                slides.push(provider.tool("inspect_slide", Some(json!({"index":index})))?);
            }
            Ok(without_runtime_ids(
                &json!({"fonts":info["fonts"],"slideSize":info["slideSize"],"slides":slides}),
            ))
        }
        _ => Err(error("Unknown installed provider")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Reader {
        calls: usize,
    }
    impl Provider for Reader {
        fn command(&mut self, app: &str, _name: &str) -> Result<Value> {
            Ok(match app {
                "writer" => {
                    json!({"pages":2,"paragraphs":10,"sections":2,"words":20,"comments":1,"trackChanges":false,"path":"ignored","dirty":false})
                }
                "sheet" => json!({"sheets":[{"name":"Sheet1","usedRange":"A1:ZZ999"}]}),
                _ => json!({}),
            })
        }
        fn tool(&mut self, name: &str, _arguments: Option<Value>) -> Result<Value> {
            self.calls += 1;
            Ok(if name == "inspect_document" {
                json!({"blocks":[{"text":"all body text","runs":[{"bold":true}]}],"selection":{"offset":2},"canUndo":true,"canRedo":false})
            } else {
                json!({"body":"all body text","header":"header text"})
            })
        }
    }
    #[test]
    fn only_opaque_ids_are_removed() {
        let original = json!({"id":1,"shapeId":9,"text":"sample","box":[1,2,3,4],"fill":{"id":2,"color":"red"}});
        assert_eq!(
            without_runtime_ids(&original),
            json!({"shapeId":9,"text":"sample","box":[1,2,3,4],"fill":{"color":"red"}})
        );
        assert_eq!(original["id"], 1);
    }
    #[test]
    fn writer_projection_retains_text_and_declared_counts() {
        let mut reader = Reader { calls: 0 };
        let result = snapshot(&mut reader, "writer").unwrap();
        assert_eq!(result["text"]["header"], "header text");
        assert_eq!(result["structure"]["comments"], 1);
        assert!(result["structure"].get("path").is_none());
        assert_eq!(
            result["document"],
            json!({"blocks":[{"text":"all body text","runs":[{"bold":true}]}]})
        );
    }
    #[test]
    fn sheet_bounds_fail_before_cell_read() {
        let mut reader = Reader { calls: 0 };
        assert!(snapshot(&mut reader, "sheet").is_err());
        assert_eq!(reader.calls, 0);
    }
}
