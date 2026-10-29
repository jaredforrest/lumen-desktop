//! Private, replaceable MPRIS players for native media-control acceptance.
use serde_json::{Map, Value as Json, json};
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const ROOT: &str = "org.mpris.MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const PATH: &str = "/org/mpris/MediaPlayer2";
const CONTROL_NAME: &str = "org.lumen.MediaFixture";
const CONTROL_PATH: &str = "/org/lumen/MediaFixture";
type AppResult<T> = std::result::Result<T, Box<dyn Error>>;

fn failed(error: impl ToString) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed(error.to_string())
}
fn owned<T>(value: T) -> AppResult<OwnedValue>
where
    T: Into<Value<'static>>,
{
    Ok(OwnedValue::try_from(value.into())?)
}
fn metadata(source: &Json) -> AppResult<HashMap<String, OwnedValue>> {
    let mut output = HashMap::new();
    for (key, value) in source.as_object().ok_or("Metadata is not an object")? {
        let wire = match key.as_str() {
            "mpris:trackid" => OwnedValue::from(ObjectPath::try_from(
                value.as_str().ok_or("Track ID is not a path")?,
            )?),
            "mpris:length" => {
                OwnedValue::from(value.as_i64().ok_or("Track length is not an integer")?)
            }
            "xesam:artist" => owned(
                value
                    .as_array()
                    .ok_or("Artists are not an array")?
                    .iter()
                    .map(|item| item.as_str().unwrap_or("").to_owned())
                    .collect::<Vec<_>>(),
            )?,
            _ => owned(value.as_str().ok_or("Metadata string missing")?.to_owned())?,
        };
        output.insert(key.clone(), wire);
    }
    Ok(output)
}
fn default_properties() -> Map<String, Json> {
    json!({
        "PlaybackStatus":"Paused","Rate":1.0,"Position":0,
        "MinimumRate":1.0,"MaximumRate":1.0,
        "CanGoNext":true,"CanGoPrevious":true,"CanPlay":true,
        "CanPause":true,"CanSeek":true,"CanControl":true,"Volume":0.5,
        "Metadata":{"mpris:trackid":"/fixture/track1","mpris:length":180000000,
            "xesam:title":"Synthetic title","xesam:artist":["Synthetic artist"]}
    })
    .as_object()
    .unwrap()
    .clone()
}
#[derive(Clone)]
struct Recorder {
    calls: Arc<Mutex<Vec<Json>>>,
    path: PathBuf,
}
impl Recorder {
    fn record(&self, name: &str, generation: u64, method: &str, args: Vec<Json>) {
        let row = json!({"name":name,"generation":generation,"method":method,"args":args});
        self.calls.lock().unwrap().push(row.clone());
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .unwrap();
        writeln!(file, "{}", row).unwrap();
    }
}
struct PlayerData {
    name: String,
    generation: u64,
    identity: String,
    properties: Mutex<Map<String, Json>>,
    recorder: Recorder,
}
impl PlayerData {
    fn get(&self, key: &str) -> Json {
        self.properties
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .unwrap_or(Json::Null)
    }
    fn text(&self, key: &str) -> String {
        self.get(key).as_str().unwrap_or("").to_owned()
    }
    fn boolean(&self, key: &str) -> bool {
        self.get(key).as_bool().unwrap_or(false)
    }
    fn real(&self, key: &str) -> f64 {
        self.get(key).as_f64().unwrap_or(0.)
    }
    fn integer(&self, key: &str) -> i64 {
        self.get(key).as_i64().unwrap_or(0)
    }
    fn configure(&self, values: &Map<String, Json>) {
        let mut properties = self.properties.lock().unwrap();
        for (key, value) in values {
            if value.is_null() {
                properties.remove(key);
            } else {
                properties.insert(key.clone(), value.clone());
            }
        }
    }
    fn record(&self, method: &str, args: Vec<Json>) {
        self.recorder
            .record(&self.name, self.generation, method, args);
    }
}
#[derive(Clone)]
struct Root(Arc<PlayerData>);
#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> String {
        self.0.identity.clone()
    }
    #[zbus(property)]
    fn desktop_entry(&self) -> &str {
        "fixture"
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}
#[derive(Clone)]
struct Player(Arc<PlayerData>);
#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.0.text("PlaybackStatus")
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        self.0.real("Rate")
    }
    #[zbus(property)]
    fn position(&self) -> i64 {
        self.0.integer("Position")
    }
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        self.0.real("MinimumRate")
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        self.0.real("MaximumRate")
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.0.boolean("CanGoNext")
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.0.boolean("CanGoPrevious")
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.0.boolean("CanPlay")
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.0.boolean("CanPause")
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.0.boolean("CanSeek")
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        self.0.boolean("CanControl")
    }
    #[zbus(property)]
    fn volume(&self) -> zbus::fdo::Result<f64> {
        self.0
            .get("Volume")
            .as_f64()
            .ok_or(zbus::fdo::Error::UnknownProperty("Volume".into()))
    }
    #[zbus(property)]
    fn set_volume(&self, value: f64) {
        self.0.record("Set", vec![json!("Volume"), json!(value)]);
        self.0
            .configure(&json!({"Volume":value}).as_object().unwrap().clone());
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        metadata(&self.0.get("Metadata")).unwrap()
    }
    async fn play(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        self.0.record("Play", vec![]);
        self.0.configure(
            &json!({"PlaybackStatus":"Playing"})
                .as_object()
                .unwrap()
                .clone(),
        );
        self.playback_status_changed(&emitter).await.map_err(failed)
    }
    async fn pause(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        self.0.record("Pause", vec![]);
        self.0.configure(
            &json!({"PlaybackStatus":"Paused"})
                .as_object()
                .unwrap()
                .clone(),
        );
        self.playback_status_changed(&emitter).await.map_err(failed)
    }
    async fn play_pause(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        self.0.record("PlayPause", vec![]);
        let next = if self.0.text("PlaybackStatus") == "Playing" {
            "Paused"
        } else {
            "Playing"
        };
        self.0
            .configure(&json!({"PlaybackStatus":next}).as_object().unwrap().clone());
        self.playback_status_changed(&emitter).await.map_err(failed)
    }
    fn next(&self) {
        self.0.record("Next", vec![]);
    }
    fn previous(&self) {
        self.0.record("Previous", vec![]);
    }
    async fn set_position(
        &self,
        track: ObjectPath<'_>,
        position: i64,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        self.0
            .record("SetPosition", vec![json!(track.as_str()), json!(position)]);
        self.0
            .configure(&json!({"Position":position}).as_object().unwrap().clone());
        Self::seeked(&emitter, position).await.map_err(failed)
    }
    fn seek(&self, offset: i64) {
        self.0.record("Seek", vec![json!(offset)]);
    }
    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;
}
struct Entry {
    data: Arc<PlayerData>,
    connection: zbus::Connection,
}
struct State {
    next_generation: u64,
    players: HashMap<String, Entry>,
}
#[derive(Clone)]
struct Control {
    state: Arc<Mutex<State>>,
    recorder: Recorder,
}
fn wire_property(key: &str, value: &Json) -> AppResult<OwnedValue> {
    Ok(match key {
        "Metadata" => owned(metadata(value)?)?,
        "Position" => OwnedValue::from(value.as_i64().ok_or("Integer property missing")?),
        "Volume" | "Rate" | "MinimumRate" | "MaximumRate" => {
            OwnedValue::from(value.as_f64().ok_or("Floating property missing")?)
        }
        "CanGoNext" | "CanGoPrevious" | "CanPlay" | "CanPause" | "CanSeek" | "CanControl" => {
            OwnedValue::from(value.as_bool().ok_or("Boolean property missing")?)
        }
        _ => owned(value.as_str().ok_or("String property missing")?.to_owned())?,
    })
}
#[interface(name = "org.lumen.MediaFixture")]
impl Control {
    async fn add(&self, value: &str) -> zbus::fdo::Result<()> {
        let config: Json = serde_json::from_str(value).map_err(failed)?;
        let short = config["name"]
            .as_str()
            .ok_or_else(|| failed("Player name missing"))?;
        let name = format!("{ROOT}.{short}");
        let generation = {
            let mut state = self.state.lock().unwrap();
            if state.players.contains_key(short) {
                return Err(failed("Duplicate player"));
            }
            state.next_generation += 1;
            state.next_generation
        };
        let data = Arc::new(PlayerData {
            name: name.clone(),
            generation,
            identity: config["identity"].as_str().unwrap_or(short).to_owned(),
            properties: Mutex::new(default_properties()),
            recorder: self.recorder.clone(),
        });
        if let Some(properties) = config["properties"].as_object() {
            data.configure(properties);
        }
        let connection = zbus::connection::Builder::session()
            .map_err(failed)?
            .name(name.as_str())
            .map_err(failed)?
            .serve_at(PATH, Root(data.clone()))
            .map_err(failed)?
            .serve_at(PATH, Player(data.clone()))
            .map_err(failed)?
            .build()
            .await
            .map_err(failed)?;
        self.state
            .lock()
            .unwrap()
            .players
            .insert(short.to_owned(), Entry { data, connection });
        Ok(())
    }
    async fn remove(&self, key: &str) -> zbus::fdo::Result<()> {
        let entry = self
            .state
            .lock()
            .unwrap()
            .players
            .remove(key)
            .ok_or_else(|| failed("Player missing"))?;
        entry
            .connection
            .release_name(entry.data.name.as_str())
            .await
            .map_err(failed)?;
        Ok(())
    }
    async fn configure(&self, key: &str, value: &str) -> zbus::fdo::Result<()> {
        let values: Json = serde_json::from_str(value).map_err(failed)?;
        let values = values
            .as_object()
            .ok_or_else(|| failed("Properties must be an object"))?;
        let (data, connection) = {
            let state = self.state.lock().unwrap();
            let entry = state
                .players
                .get(key)
                .ok_or_else(|| failed("Player missing"))?;
            (entry.data.clone(), entry.connection.clone())
        };
        data.configure(values);
        let mut changed = HashMap::new();
        let mut invalidated = Vec::new();
        for (key, value) in values {
            if value.is_null() {
                invalidated.push(key.clone());
            } else {
                changed.insert(key.clone(), wire_property(key, value).map_err(failed)?);
            }
        }
        connection
            .emit_signal(
                None::<&str>,
                PATH,
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &(PLAYER, changed, invalidated),
            )
            .await
            .map_err(failed)?;
        Ok(())
    }
    fn calls(&self) -> String {
        serde_json::to_string(&*self.recorder.calls.lock().unwrap()).unwrap()
    }
}
#[tokio::main]
async fn main() -> AppResult<()> {
    let mut arguments = env::args_os().skip(1);
    if arguments.next().is_some() {
        return Err("Unexpected MPRIS fixture argument".into());
    }
    let path = PathBuf::from(env::var("LUMEN_MPRIS_CALLS")?);
    let recorder = Recorder {
        calls: Arc::new(Mutex::new(Vec::new())),
        path,
    };
    let control = Control {
        state: Arc::new(Mutex::new(State {
            next_generation: 0,
            players: HashMap::new(),
        })),
        recorder,
    };
    let _connection = zbus::connection::Builder::session()?
        .name(CONTROL_NAME)?
        .serve_at(CONTROL_PATH, control)?
        .build()
        .await?;
    std::future::pending::<()>().await;
    Ok(())
}
