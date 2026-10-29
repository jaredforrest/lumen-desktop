use serde_json::{json, Value};
use std::io::{self, Read};

fn latest_state_frame(frames: &[Value]) -> Result<Value, String> {
    for frame in frames.iter().rev() {
        let object = frame
            .as_object()
            .ok_or("Expected an object transport frame")?;
        if object.get("liveness") != Some(&Value::Bool(true)) {
            return Ok(frame.clone());
        }
    }
    Ok(json!({}))
}

fn run() -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())?;
    let frames: Value = serde_json::from_str(&input).map_err(|error| error.to_string())?;
    let frames = frames
        .as_array()
        .ok_or("Expected an array of transport frames")?;
    let state = latest_state_frame(frames)?;
    println!("{state}");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_does_not_replace_a_redacted_update() {
        let first = json!({"instance":"owner","revision":8,"rows":[{"id":1}]});
        let second = json!({"instance":"owner","revision":9,"rows":[{"id":1},{"id":2}]});
        let heartbeat = json!({"instance":"owner","liveness":true,"commandUntil":123});
        let mut frames = vec![first, second.clone()];
        frames.extend(std::iter::repeat_n(heartbeat.clone(), 40));
        assert_eq!(latest_state_frame(&frames), Ok(second));
        assert_eq!(latest_state_frame(&[heartbeat]), Ok(json!({})));
    }

    #[test]
    fn later_failure_and_new_owner_replace_old_rows() {
        let ready = json!({"instance":"old","rows":[{"id":1},{"id":2}]});
        for next in [
            json!({"available":false,"error":"owner lost"}),
            json!({"instance":"new","rows":[]}),
            json!({"liveness":false}),
        ] {
            assert_eq!(
                latest_state_frame(&[ready.clone(), next.clone(), json!({"liveness":true})]),
                Ok(next)
            );
        }
    }

    #[test]
    fn malformed_frame_is_not_skipped() {
        assert_eq!(
            latest_state_frame(&[json!({"rows":[1,2]}), json!([])]),
            Err("Expected an object transport frame".to_owned())
        );
    }
}
