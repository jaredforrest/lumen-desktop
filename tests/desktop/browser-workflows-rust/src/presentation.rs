use std::collections::BTreeMap;

pub const KEYS: [&str; 3] = ["FONTCONFIG_FILE", "XDG_CURRENT_DESKTOP", "GTK_USE_PORTAL"];
pub fn start_preserving_client<T, E>(
    environment: &mut BTreeMap<String, String>,
    start: impl FnOnce(&mut BTreeMap<String, String>) -> std::result::Result<T, E>,
) -> std::result::Result<T, E> {
    let saved: Vec<_> = KEYS
        .iter()
        .map(|key| ((*key).to_string(), environment.get(*key).cloned()))
        .collect();
    let result = start(environment);
    for (key, value) in saved {
        if let Some(value) = value {
            environment.insert(key, value);
        } else {
            environment.remove(&key);
        }
    }
    result
}
