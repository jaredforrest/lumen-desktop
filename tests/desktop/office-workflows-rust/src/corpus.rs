//! Synthetic structured corpus for installed office providers.
use crate::Result;
use serde_json::Value;

pub struct Corpus {
    pub image: &'static [u8],
    pub writer: Vec<(String, Value)>,
    pub grid: Vec<(String, Value)>,
    pub deck: Vec<(String, Value)>,
}

pub fn create() -> Result<Corpus> {
    let data: Value = serde_json::from_str(include_str!("../corpus.json"))?;
    let actions = |key: &str| -> Result<Vec<(String, Value)>> {
        Ok(data[key]
            .as_array()
            .ok_or("Corpus action list missing")?
            .iter()
            .map(|item| {
                Ok((
                    item[0]
                        .as_str()
                        .ok_or("Corpus action name missing")?
                        .to_owned(),
                    item[1].clone(),
                ))
            })
            .collect::<Result<Vec<_>>>()?)
    };
    Ok(Corpus {
        image: include_bytes!("../synthetic.png"),
        writer: actions("writer")?,
        grid: actions("grid")?,
        deck: actions("deck")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn corpus_image_and_action_payload_are_synthetic() {
        let corpus = create().unwrap();
        let decoder = png::Decoder::new(Cursor::new(corpus.image));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!((info.width, info.height), (32, 32));
        assert_eq!(&pixels[..3], &[224, 48, 80]);
        assert!(corpus.writer.len() > 20 && corpus.grid.len() > 10 && corpus.deck.len() > 5);
        assert!(corpus.writer.iter().all(|(_, params)| params.is_object()));
    }
}
