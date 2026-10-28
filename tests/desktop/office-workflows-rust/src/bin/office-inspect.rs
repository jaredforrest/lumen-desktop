use lumen_office_workflows::formats;
use std::path::Path;

fn main() -> lumen_office_workflows::Result<()> {
    let mut args = std::env::args().skip(1);
    let kind = args.next().ok_or("Usage: office-inspect KIND PATH")?;
    let path = args.next().ok_or("Missing path")?;
    let facts = match kind.as_str() {
        "writer" => formats::writer(Path::new(&path))?,
        "sheet" => formats::spreadsheet(Path::new(&path))?,
        "slides" => formats::presentation(Path::new(&path))?,
        _ => return Err("Expected writer, sheet, or slides".into()),
    };
    println!("{}", serde_json::to_string(&facts)?);
    Ok(())
}
