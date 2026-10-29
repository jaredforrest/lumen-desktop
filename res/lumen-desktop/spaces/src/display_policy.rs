//! Display identity, preference, and observer projection shared by the desktop shell.
use serde_json::{Value, json};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::PathBuf;

type Result<T> = std::result::Result<T, String>;

fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|word| !word.is_empty() && word.chars().count() <= 512
        && !word.chars().any(char::is_control))
}

pub fn identity_record(key: &str) -> Result<Value> {
    if let Some(name) = key.strip_prefix("connector:") {
        if text(&json!(name)).is_some() {
            return Ok(json!({"kind":"connector","name":name,"make":null,"model":null,"serial":null}));
        }
    }
    if let Some(encoded) = key.strip_prefix("display:") {
        let fields: Value = serde_json::from_str(encoded).map_err(|_| "Invalid display identity metadata")?;
        let Some(fields) = fields.as_array().filter(|fields| fields.len() == 3) else {
            return Err("Invalid typed display identity".into());
        };
        if fields.iter().all(|field| text(field).is_some())
            && !["Unknown", "0"].contains(&fields[2].as_str().unwrap_or(""))
            && serde_json::to_string(fields).map_err(|error| error.to_string())? == encoded {
            return Ok(json!({"kind":"display","name":null,"make":fields[0],"model":fields[1],"serial":fields[2]}));
        }
    }
    Err("Invalid typed display identity".into())
}

pub fn identity_key(record: &Value) -> Result<Option<String>> {
    if record.is_null() { return Ok(None); }
    let Some(fields) = record.as_object() else { return Err("Invalid primary display identity".into()); };
    if fields.keys().any(|key| !["kind","name","make","model","serial"].contains(&key.as_str())) {
        return Err("Invalid primary display identity".into());
    }
    let key = match record["kind"].as_str() {
        Some("connector") if text(&record["name"]).is_some()
            && ["make","model","serial"].iter().all(|name| record[*name].is_null()) => {
            format!("connector:{}", record["name"].as_str().unwrap())
        }
        Some("display") if record["name"].is_null()
            && ["make","model","serial"].iter().all(|name| text(&record[*name]).is_some()) => {
            format!("display:{}", json!([record["make"],record["model"],record["serial"]]))
        }
        _ => return Err("Invalid primary display identity".into()),
    };
    identity_record(&key)?;
    Ok(Some(key))
}

pub fn preferences(value: &Value) -> Result<Value> {
    let fields = value.as_object().ok_or("Invalid display-space preferences")?;
    if fields.keys().any(|key| !["separateDisplays","primaryDisplay"].contains(&key.as_str())) {
        return Err("Invalid display-space preferences".into());
    }
    let separate = value.get("separateDisplays").unwrap_or(&Value::Bool(true));
    let separate = separate.as_bool().ok_or("Display separation must be boolean")?;
    let primary = identity_key(&value["primaryDisplay"])?;
    Ok(json!({"separateDisplays":separate,"primaryDisplay":primary}))
}

pub fn display_identity(output: &Value) -> Result<String> {
    let parts = ["make","model","serial"].map(|key| output[key].clone());
    if parts.iter().all(|part| text(part).is_some())
        && !["Unknown", "0"].contains(&parts[2].as_str().unwrap_or("")) {
        return Ok(format!("display:{}", json!(parts)));
    }
    let name = text(&output["name"]).ok_or("Invalid display connector identity")?;
    Ok(format!("connector:{name}"))
}

fn settings_path() -> Vec<PathBuf> {
    if let Some(active) = env::var_os("LUMEN_NIX_SETTINGS_ACTIVE") {
        return vec![PathBuf::from(active)];
    }
    let runtime = env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })));
    vec![PathBuf::from("/etc/nix/desktop-settings.json"), runtime.join("lumen-nix-settings/current/settings.json")]
}

pub fn configured() -> Result<Value> {
    for path in settings_path() {
        if !path.exists() { continue; }
        let bytes = fs::read(&path).map_err(|error| error.to_string())?;
        if bytes.len() > 4 * 1024 * 1024 { return Err("Display preferences exceed the settings read budget".into()); }
        let value: Value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if !value.is_object() { return Err("Invalid settings object".into()); }
        return preferences(value.get("spaces").unwrap_or(&json!({})));
    }
    preferences(&json!({}))
}

pub fn remembered_primary() -> Result<Option<String>> {
    let state = env::var_os("XDG_STATE_HOME").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    let path = state.join("lumen-desktop/spaces.json");
    if !path.exists() { return Ok(None); }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 { return Err("Display state exceeds the observation budget".into()); }
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if !value.is_object() { return Err("Invalid saved display state".into()); }
    let Some(key) = value["automatic_primary"].as_str() else { return Ok(None); };
    identity_record(key)?;
    Ok(Some(key.to_owned()))
}

pub fn primary<'a>(preferred: Option<&str>, remembered: Option<&str>, outputs: &'a [Value]) -> Result<Option<&'a Value>> {
    let mut active = Vec::new();
    let mut keys = HashSet::new();
    for output in outputs.iter().filter(|output| output["active"] != false) {
        let key = display_identity(output)?;
        if !keys.insert(key) { return Err("Ambiguous display identity".into()); }
        active.push(output);
    }
    let wanted = preferred.or(remembered);
    if let Some(wanted) = wanted {
        identity_record(wanted)?;
        if let Some(row) = active.iter().find(|row| display_identity(row).ok().as_deref() == Some(wanted)) {
            return Ok(Some(row));
        }
        if let Some(name) = wanted.strip_prefix("connector:") {
            if let Some(row) = active.iter().find(|row| row["name"] == name) { return Ok(Some(row)); }
        }
    }
    active.sort_by_key(|row| (row["rect"]["x"].as_i64().unwrap_or(0),
        row["rect"]["y"].as_i64().unwrap_or(0), display_identity(row).unwrap_or_default()));
    Ok(active.first().copied())
}

pub fn project(native: &Value, outputs: &[Value], desired: &Value, remembered: Option<&str>) -> Result<Value> {
    let separate = desired["separateDisplays"] == true;
    let preferred = desired["primaryDisplay"].as_str();
    let mut rows = Vec::new();
    for output in outputs.iter().filter(|output| output["active"] == true) {
        let key = display_identity(output)?;
        rows.push(json!({"name":output["name"],"identity":identity_record(&key)?,
            "label":output["model"].as_str().filter(|word| !word.is_empty()).unwrap_or(output["name"].as_str().unwrap_or(""))}));
    }
    let record = preferred.map(identity_record).transpose()?.unwrap_or(Value::Null);
    if native.is_null() {
        return Ok(json!({"protocolVersion":1,"nativeSupported":false,"session":null,"generation":null,
            "available":false,"applied":false,"independent":true,"separateDisplays":separate,
            "primary":"","primaryDisplay":record,"outputs":rows,
            "error":"Display policy requires the updated compositor; using independent Dock routing"}));
    }
    let generation = native["generation"].as_str().ok_or("Updated display policy is unavailable")?;
    if native["version"] != 1 || native["session"].as_str().is_none_or(|s| s.len()!=32 || !s.bytes().all(|b|b.is_ascii_hexdigit() && !b.is_ascii_uppercase()))
        || !native["available"].is_boolean() || !native["separateDisplays"].is_boolean()
        || generation.parse::<u64>().is_err() || generation != generation.parse::<u64>().unwrap().to_string() {
        return Err("Updated display policy is unavailable".into());
    }
    let available = native["available"] == true && rows.iter().any(|row| row["name"] == native["primary"]);
    let chosen = primary(preferred, remembered, outputs)?;
    let applied = available && native["separateDisplays"] == desired["separateDisplays"]
        && chosen.is_some_and(|row| row["name"] == native["primary"]);
    Ok(json!({"protocolVersion":1,"nativeSupported":true,"session":native["session"],"generation":generation,
        "available":available,"applied":applied,"independent":separate && native["separateDisplays"] == true,
        "separateDisplays":separate,"primary":if available { native["primary"].as_str().unwrap_or("") } else { "" },
        "primaryDisplay":record,"outputs":rows,
        "error":if applied { "" } else { "Display policy is waiting for a current native registration" }}))
}

/// Reconcile one logical member per connected output without moving windows.
pub fn plan(mut previous: Vec<Value>, desktops: &[Value], outputs: &[Value], preserve_visible: bool,
    mut new_id: impl FnMut() -> String) -> Result<Value> {
    const LIMIT: usize = 64;
    if desktops.len() > LIMIT { return Err("The desktop limit is 64".into()); }
    if outputs.len() > LIMIT || previous.len() > LIMIT { return Err("Invalid display context budget".into()); }
    let mut keys = HashSet::<String>::new();
    for desktop in desktops {
        let key = text(&desktop["logicalId"]).ok_or("Invalid logical desktop identities")?;
        if !desktop.is_object() || !keys.insert(key.to_owned()) { return Err("Invalid logical desktop identities".into()); }
    }
    let mut output_keys = Vec::<String>::new();
    let mut active = std::collections::HashMap::<String, Vec<&Value>>::new();
    for output in outputs.iter().filter(|output| output["active"] != false) {
        if !output.is_object() { return Err("Invalid display list".into()); }
        let key = display_identity(output)?;
        if active.contains_key(&key) { return Err("Ambiguous display identity".into()); }
        active.insert(key.clone(), desktops.iter().filter(|desktop| desktop["outputId"] == key).collect());
        output_keys.push(key);
    }
    let mut old_ids = HashSet::new();
    for cohort in &previous {
        let id = text(&cohort["id"]).ok_or("Invalid cohort identity")?;
        if !old_ids.insert(id.to_owned()) { return Err("Invalid cohort identity".into()); }
        let members = cohort["members"].as_object().ok_or("Invalid cohort members")?;
        if members.len() > LIMIT { return Err("Invalid cohort members".into()); }
        for (output, desktop) in members {
            identity_record(output)?;
            text(desktop).ok_or("Invalid cohort member")?;
        }
    }
    let count = active.values().map(Vec::len).max().unwrap_or(0);
    if count == 0 {
        let cohorts: Vec<Value> = previous.iter().filter_map(|cohort| {
            let members = cohort["members"].as_object().unwrap().iter()
                .filter(|(_,key)| key.as_str().is_some_and(|key| keys.contains(key)))
                .map(|(output,key)| (output.clone(),key.clone()))
                .collect::<serde_json::Map<String,Value>>();
            (!members.is_empty()).then(|| json!({"id":cohort["id"],"members":members}))
        }).collect();
        return Ok(json!({"cohorts":cohorts,"created":[],"provenance":previous}));
    }
    let mut cohorts: Vec<Value> = previous.iter().take(count).map(|cohort| json!({"id":cohort["id"],"members":{}})).collect();
    while cohorts.len() < count {
        let key = new_id();
        if text(&json!(key)).is_none() || cohorts.iter().any(|cohort| cohort["id"] == key) {
            return Err("Invalid generated cohort identity".into());
        }
        cohorts.push(json!({"id":key,"members":{}}));
    }
    let mut assigned = HashSet::<String>::new();
    if preserve_visible {
        for output in &output_keys {
            let visible: Vec<&&Value> = active[output].iter().filter(|row| row["visible"] == true).collect();
            if visible.len() != 1 { return Err("A connected display has no unique visible desktop".into()); }
            let key = visible[0]["logicalId"].as_str().unwrap().to_owned();
            cohorts[0]["members"][output] = json!(key);
            assigned.insert(key);
        }
    }
    for (index, cohort) in cohorts.iter_mut().enumerate() {
        let Some(old) = previous.get(index) else { continue; };
        for output in &output_keys {
            let Some(candidate) = old["members"][output].as_str() else { continue; };
            if cohort["members"][output].is_null() && !assigned.contains(candidate)
                && active[output].iter().any(|row| row["logicalId"] == candidate) {
                cohort["members"][output] = json!(candidate);
                assigned.insert(candidate.to_owned());
            }
        }
    }
    for output in &output_keys {
        let remaining = active[output].iter()
            .filter_map(|row| row["logicalId"].as_str())
            .filter(|key| !assigned.contains(*key)).map(ToOwned::to_owned).collect::<Vec<_>>();
        let mut remaining = remaining.into_iter();
        for cohort in &mut cohorts {
            if cohort["members"][output].is_null() {
                if let Some(key) = remaining.next() {
                    cohort["members"][output] = json!(key);
                    assigned.insert(key);
                }
            }
        }
    }
    let missing = cohorts.iter().flat_map(|cohort| output_keys.iter().filter(move |output| cohort["members"][*output].is_null())).count();
    if desktops.len() + missing > LIMIT {
        return Err("Coupled desktops require more than the 64-desktop budget; existing desktops are unchanged".into());
    }
    let mut created = Vec::new();
    for cohort in &mut cohorts {
        for output in &output_keys {
            if cohort["members"][output].is_null() {
                let key = new_id();
                if text(&json!(key)).is_none() || !keys.insert(key.clone()) {
                    return Err("Invalid generated desktop identity".into());
                }
                cohort["members"][output] = json!(key);
                created.push(json!({"logicalId":key,"outputId":output,"name":format!("lumen:{key}"),
                    "persistent":true,"generated":true}));
            }
        }
    }
    let known: HashSet<&str> = desktops.iter().filter_map(|row| row["logicalId"].as_str()).collect();
    let mut retained: HashSet<String> = cohorts.iter().flat_map(|cohort| cohort["members"].as_object().into_iter().flat_map(|m|m.values()))
        .filter_map(|key|key.as_str().map(ToOwned::to_owned)).collect();
    for before in &previous {
        let id = before["id"].as_str().unwrap();
        for (output,key) in before["members"].as_object().unwrap() {
            let key = key.as_str().unwrap();
            if output_keys.contains(output) || !known.contains(key) || retained.contains(key) { continue; }
            let target = if let Some(index) = cohorts.iter().position(|cohort| cohort["id"] == id) { index }
                else { cohorts.push(json!({"id":id,"members":{}})); cohorts.len()-1 };
            cohorts[target]["members"][output] = json!(key);
            retained.insert(key.to_owned());
        }
    }
    Ok(json!({"cohorts":cohorts,"created":created,"provenance":std::mem::take(&mut previous)}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_identity_survives_disconnection_with_deterministic_fallback() {
        let outputs = vec![json!({"name":"B","make":"Acme","model":"Panel","serial":"42","active":true,"rect":{"x":200,"y":0}}),
            json!({"name":"A","active":true,"rect":{"x":0,"y":0}})];
        let desired = preferences(&json!({"separateDisplays":false,"primaryDisplay":{"kind":"connector","name":"Gone"}})).unwrap();
        let native = json!({"version":1,"session":"0123456789abcdef0123456789abcdef","generation":"1","available":true,"separateDisplays":false,"primary":"A"});
        let value = project(&native, &outputs, &desired, None).unwrap();
        assert_eq!(value["applied"], true);
        assert_eq!(value["primaryDisplay"]["name"], "Gone");
        assert_eq!(value["primary"], "A");
        assert_eq!(value["outputs"][0]["identity"]["kind"], "display");
    }

    #[test]
    fn coupled_planner_preserves_visible_composition_and_refuses_duplicate_ids() {
        let outputs = vec![json!({"name":"A","active":true}),json!({"name":"B","active":true})];
        let desktops = vec![json!({"logicalId":"a1","outputId":"connector:A","visible":false}),
            json!({"logicalId":"a2","outputId":"connector:A","visible":true}),
            json!({"logicalId":"b1","outputId":"connector:B","visible":true})];
        let mut ids = ["cohort1","cohort2","new-desktop"].into_iter();
        let result = plan(vec![], &desktops, &outputs, true, || ids.next().unwrap().into()).unwrap();
        assert_eq!(result["cohorts"][0]["members"]["connector:A"], "a2");
        assert_eq!(result["cohorts"][0]["members"]["connector:B"], "b1");
        assert_eq!(result["created"].as_array().unwrap().len(), 1);
        assert!(plan(vec![], &desktops, &outputs, false, || "same".into()).is_err());
    }

    #[test]
    fn display_identity_and_primary_selection_are_typed_and_stable() {
        assert_eq!(display_identity(&json!({"name":"A","make":"Maker","model":"Écran","serial":"abc"})).unwrap(),
            "display:[\"Maker\",\"Écran\",\"abc\"]");
        assert!(identity_record("connector:A\n").is_err());
        assert!(identity_record("display:[\"x\", \"y\", \"1\"]").is_err());
        assert!(identity_record("display:[\"x\",\"y\",\"0\"]").is_err());
        let outputs=vec![json!({"name":"right","active":true,"focused":true,"rect":{"x":100}}),
            json!({"name":"left","active":true,"rect":{"x":0}})];
        assert_eq!(primary(None,None,&outputs).unwrap().unwrap()["name"],"left");
        assert_eq!(primary(Some("connector:right"),None,&outputs).unwrap().unwrap()["name"],"right");
        assert!(primary(None,None,&[outputs[0].clone(),outputs[0].clone()]).is_err());
        let enriched=json!({"name":"right","make":"Maker","model":"Panel","serial":"123","active":true});
        assert_eq!(primary(Some("connector:right"),None,&[enriched]).unwrap().unwrap()["name"],"right");
    }

    #[test]
    fn coupled_plan_repackages_after_output_loss_and_rejects_capacity_growth() {
        let old=vec![json!({"id":"one","members":{"connector:A":"a","connector:B":"b"}})];
        let rows=vec![json!({"logicalId":"a","outputId":"connector:A","visible":true}),
            json!({"logicalId":"b","outputId":"connector:A","visible":false})];
        let result=plan(old.clone(),&rows,&[json!({"name":"A","active":true})],false,||"new".into()).unwrap();
        assert_eq!(result["created"],json!([]));
        assert_eq!(result["provenance"],json!(old));
        let members=result["cohorts"].as_array().unwrap().iter().map(|row|row["members"]["connector:A"].as_str().unwrap()).collect::<HashSet<_>>();
        assert_eq!(members,HashSet::from(["a","b"]));
        let crowded=(0..63).map(|index|json!({"logicalId":index.to_string(),"outputId":"connector:A","visible":index==0})).chain(
            std::iter::once(json!({"logicalId":"b","outputId":"connector:B","visible":true}))).collect::<Vec<_>>();
        assert!(plan(vec![],&crowded,&[json!({"name":"A","active":true}),json!({"name":"B","active":true})],true,
            ||uuid::Uuid::new_v4().to_string()).unwrap_err().contains("budget"));
    }

    #[test]
    fn malformed_display_authority_and_preferences_are_rejected() {
        let outputs = vec![json!({"name":"A","active":true,"model":"Panel"})];
        let desired = preferences(&json!({})).unwrap();
        let native = json!({"version":1,"session":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","generation":"0",
            "available":true,"primary":"A","separateDisplays":true});
        let result=project(&native,&outputs,&desired,None).unwrap();
        assert_eq!(result["applied"],true);
        assert_eq!(result["independent"],true);
        for (field,value) in [("version",json!(true)),("session",json!("unknown")),
            ("generation",json!(1)),("generation",json!("01")),
            ("generation",json!("18446744073709551616")),("separateDisplays",json!("false")),
            ("available",json!(1))] {
            let mut bad=native.clone();bad[field]=value;
            assert!(project(&bad,&outputs,&desired,None).is_err(),"{field}: {bad}");
        }
        for bad in [json!({"separateDisplays":1}),json!({"primaryDisplay":"connector:A"}),
            json!({"primaryDisplay":{"kind":"connector","name":"A","serial":"1"}}),
            json!({"primaryDisplay":{"kind":"display","make":"M","model":"P","serial":"Unknown"}})] {
            assert!(preferences(&bad).is_err(),"{bad}");
        }
    }

    #[test]
    fn planning_retains_disconnected_members_and_repackages_migrated_desktops() {
        let old=vec![json!({"id":"one","members":{"connector:A":"a","connector:B":"b"}})];
        let outputs=vec![json!({"name":"A","active":true})];
        let same=vec![json!({"logicalId":"a","outputId":"connector:A","visible":true}),
            json!({"logicalId":"b","outputId":"connector:B","visible":false})];
        let retained=plan(old.clone(),&same,&outputs,false,||"new".into()).unwrap();
        assert_eq!(retained["cohorts"],json!(old));
        let migrated=vec![json!({"logicalId":"a","outputId":"connector:A","visible":true}),
            json!({"logicalId":"b","outputId":"connector:A","visible":false})];
        let result=plan(old.clone(),&migrated,&outputs,false,||"new-context".into()).unwrap();
        let assigned=result["cohorts"].as_array().unwrap().iter().map(|row|row["members"]["connector:A"].as_str().unwrap()).collect::<HashSet<_>>();
        assert_eq!(assigned,HashSet::from(["a","b"]));
        assert_eq!(result["created"],json!([]));
        assert_eq!(result["provenance"],json!(old));
    }

    #[test]
    fn planning_refuses_ambiguous_visibility_without_mutating_inputs() {
        let outputs=vec![json!({"name":"A","active":true})];
        let desktops=vec![json!({"logicalId":"a","outputId":"connector:A","visible":true}),
            json!({"logicalId":"b","outputId":"connector:A","visible":true})];
        let original=desktops.clone();
        assert!(plan(vec![],&desktops,&outputs,true,||"new".into()).is_err());
        assert_eq!(desktops,original);
        let stable=vec![json!({"id":"one","members":{"connector:A":"a"}})];
        assert_eq!(plan(stable.clone(),&desktops[..1],&outputs,false,||"new".into()).unwrap()["cohorts"],json!(stable));
        assert_eq!(plan(vec![json!({"id":"one","members":{"connector:A":"retired"}})],&[],&[] ,false,||"unused".into()).unwrap()["cohorts"],json!([]));
    }

    #[test]
    fn projection_never_claims_unregistered_or_disconnected_primary() {
        let outputs=vec![json!({"name":"A","active":true,"rect":{"x":0,"y":0}}),
            json!({"name":"B","active":true,"rect":{"x":100,"y":0}})];
        let desired=preferences(&json!({"separateDisplays":false,"primaryDisplay":{"kind":"connector","name":"B"}})).unwrap();
        let native=json!({"version":1,"session":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","generation":"0",
            "available":true,"primary":"A","separateDisplays":false});
        let pending=project(&native,&outputs,&desired,None).unwrap();
        assert_eq!(pending["available"],true);
        assert_eq!(pending["applied"],false);
        let matched=project(&json!({"version":1,"session":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","generation":"0",
            "available":true,"primary":"B","separateDisplays":false}),&outputs,&desired,None).unwrap();
        assert_eq!(matched["applied"],true);
        let disconnected=project(&native,&outputs[1..],&desired,None).unwrap();
        assert_eq!(disconnected["available"],false);
        assert_eq!(disconnected["primary"],"");
        let legacy=project(&Value::Null,&outputs,&desired,None).unwrap();
        assert_eq!(legacy["nativeSupported"],false);
        assert_eq!(legacy["independent"],true);
        assert_eq!(legacy["applied"],false);
    }
}
