//! Durable desktop cohorts and captured compositor registration.
use crate::{Policy, Result, display_policy};
use serde_json::{Value, json};
use std::collections::HashMap;

fn active_outputs(outputs: &[Value]) -> Result<Vec<(&Value, String)>> {
    outputs.iter().filter(|row| row["active"] != false)
        .map(|row| Ok((row, display_policy::display_identity(row)?))).collect()
}

fn hex(value: &Value) -> String {
    value.to_string().as_bytes().iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn synchronize(policy: &mut Policy, query: &mut impl FnMut(&[&str]) -> Result<Value>, desired: Option<Value>) -> Result<Value> {
    let desired = match desired { Some(value) => display_policy::preferences(&value)?, None => display_policy::configured()? };
    let outputs = query(&["-t", "get_outputs"])?;
    let outputs = outputs.as_array().ok_or("Invalid connected displays")?;
    let tree = query(&["-t", "get_tree"])?;
    let native = &tree["display_spaces"];
    if native["version"] != 1 || native["session"].as_str().is_none() {
        return Ok(json!({"available":false,"separateDisplays":desired["separateDisplays"],"applied":false,
            "primary":null,"error":"Display-space policy requires the updated compositor"}));
    }
    let active = active_outputs(outputs)?;
    let remembered = policy.state["automatic_primary"].as_str();
    let chosen = display_policy::primary(desired["primaryDisplay"].as_str(), remembered, outputs)?;
    let Some(chosen) = chosen else {
        return Ok(json!({"available":false,"separateDisplays":desired["separateDisplays"],"applied":false,
            "primary":null,"error":"No connected main display is available"}));
    };
    let chosen_name = chosen["name"].as_str().ok_or("Invalid main display")?;
    let chosen_key = display_policy::display_identity(chosen)?;
    let preferred = desired["primaryDisplay"].as_str().or(remembered).unwrap_or(&chosen_key).to_owned();
    let before = policy.state.clone();
    policy.state["display_separate"] = desired["separateDisplays"].clone();
    if policy.state["automatic_primary"].is_null() {
        let automatic = display_policy::primary(None, None, outputs)?.ok_or("No connected main display is available")?;
        policy.state["automatic_primary"] = json!(display_policy::display_identity(automatic)?);
    }
    if desired["separateDisplays"] == false {
        let prior = policy.state["display_cohorts"].as_array().cloned().unwrap_or_default();
        let planned = display_policy::plan(prior.clone(), &policy.catalog(), outputs,
            native["coupled"] != true || prior.is_empty(), || uuid::Uuid::new_v4().to_string())?;
        if planned["cohorts"] != policy.state["display_cohorts"] {
            policy.state["display_cohort_provenance"] = planned["provenance"].clone();
        }
        policy.state["display_cohorts"] = planned["cohorts"].clone();
        for created in planned["created"].as_array().into_iter().flatten() {
            let count = policy.state["created_counter"].as_u64().unwrap_or(0) + 1;
            policy.state["created_counter"] = json!(count);
            let id = created["logicalId"].as_str().ok_or("Invalid generated desktop identity")?;
            let output_id = created["outputId"].as_str().ok_or("Invalid generated output identity")?;
            let output = policy.outputs.get(output_id).ok_or("The desktop display is disconnected")?;
            let ordinal = policy.state["desktops"].as_object().into_iter().flat_map(|items|items.values())
                .filter_map(|item|item["ordinal"].as_i64()).max().unwrap_or(-1) + 1;
            policy.state["desktops"][id] = json!({"name":created["name"],"output":output,
                "output_id":output_id,"persistent":true,"generated":true,"label":format!("Desktop {count}"),
                "ordinal":ordinal,"recent":0});
        }
    }
    if policy.state != before { policy.save()?; }
    let mut cohorts = Vec::new();
    let by_native: HashMap<&str, i64> = policy.current.iter().map(|(id,key)|(key.as_str(),*id)).collect();
    if desired["separateDisplays"] == false {
        for cohort in policy.state["display_cohorts"].as_array().into_iter().flatten() {
            let mut members = Vec::new();
            for (output_id,key) in cohort["members"].as_object().ok_or("Invalid cohort members")? {
                let Some(output_name) = policy.outputs.get(output_id) else { continue; };
                let key = key.as_str().ok_or("Invalid cohort member")?;
                let item = &policy.state["desktops"][key];
                if item.is_null() { return Err("A coupled desktop was retired; refresh before applying".into()); }
                members.push(json!({"output":output_name,"name":item["name"],
                    "id":by_native.get(key).map(ToString::to_string)}));
            }
            if !members.is_empty() {
                if members.len() != active.len() { return Err("Incomplete display context; existing desktops are unchanged".into()); }
                cohorts.push(json!({"id":cohort["id"],"members":members}));
            }
        }
    }
    let captured: Vec<Value> = active.iter().map(|(row,_)| json!({"id":row["id"].as_u64().map(|id|id.to_string()),
        "name":row["name"],"x":row["rect"]["x"],"y":row["rect"]["y"],
        "width":row["rect"]["width"],"height":row["rect"]["height"]})).collect();
    let packet = json!({"version":1,"session":native["session"],"separateDisplays":desired["separateDisplays"],
        "primary":chosen_name,"outputs":captured,"cohorts":cohorts});
    if packet.to_string().len() > 65536 { return Err("Display contexts exceed the native configuration budget".into()); }
    if policy.display_packet.as_ref() != Some(&packet) || native["available"] != true
        || native["primary"] != chosen_name || native["separateDisplays"] != desired["separateDisplays"]
        || policy.display_generation.as_ref() != Some(&native["generation"]) {
        let mut wire = packet.clone();
        wire["expectedGeneration"] = native["generation"].clone();
        let response = query(&[&format!("lumen_window display-spaces configure {}", hex(&wire))])?;
        if !response.as_array().is_some_and(|rows| !rows.is_empty() && rows.iter().all(|row| row["success"] == true)) {
            return Err("Display bindings changed; the captured policy was not applied".into());
        }
        policy.display_packet = Some(packet);
        let refreshed = query(&["-t", "get_tree"])?;
        if !refreshed["display_spaces"].is_object() { return Err("Invalid native display registration".into()); }
        policy.display_generation = Some(refreshed["display_spaces"]["generation"].clone());
        let native = &refreshed["display_spaces"];
        return Ok(status(native, &desired, chosen_name, &preferred, &chosen_key)?);
    }
    status(native, &desired, chosen_name, &preferred, &chosen_key)
}

fn status(native: &Value, desired: &Value, chosen_name: &str, preferred: &str, effective: &str) -> Result<Value> {
    let available = native["available"] == true;
    let applied = available && native["separateDisplays"] == desired["separateDisplays"] && native["primary"] == chosen_name;
    let fallback = preferred.strip_prefix("connector:").map_or(preferred != effective, |name| name != chosen_name);
    Ok(json!({"available":available,"separateDisplays":desired["separateDisplays"],"applied":applied,
        "primary":native["primary"],"primaryIdentity":display_policy::identity_record(preferred)?,
        "fallback":fallback,"coupled":native["coupled"] == true,
        "error":if applied { "" } else { "Display registration was superseded or is unavailable" }}))
}

pub fn activation(policy: &Policy, desktop: &Value, query: &mut impl FnMut(&[&str]) -> Result<Value>) -> Result<Option<String>> {
    let tree = query(&["-t", "get_tree"])?;
    let native = &tree["display_spaces"];
    if !native.is_object() { return Err("Invalid native display registration".into()); }
    if native["coupled"] != true { return Ok(None); }
    if native["available"] != true || policy.state["display_separate"] != false {
        return Err("Display contexts changed; refresh before switching".into());
    }
    let output_id = desktop["outputId"].as_str().ok_or("Invalid desktop output")?;
    let logical_id = desktop["logicalId"].as_str().ok_or("Invalid desktop identity")?;
    let matching: Vec<&Value> = policy.state["display_cohorts"].as_array().into_iter().flatten()
        .filter(|row| row["members"][output_id] == logical_id).collect();
    if matching.len() != 1 { return Err("This desktop has no unique display context".into()); }
    let session = native["session"].as_str().filter(|value| !value.is_empty()).ok_or("Display selection is unavailable")?;
    let generation = native["generation"].as_str().filter(|value| !value.is_empty()).ok_or("Display selection is unavailable")?;
    let cohort = matching[0]["id"].as_str().filter(|value| !value.is_empty()).ok_or("Display selection is unavailable")?;
    let output = desktop["output"].as_str().filter(|value| !value.is_empty()).ok_or("Display selection is unavailable")?;
    let packet = json!({"session":session,"expectedGeneration":generation,"cohort":cohort,"output":output,
        "workspace":desktop["name"],"workspaceId":desktop["id"].as_i64().map(|id|id.to_string())});
    Ok(Some(format!("lumen_window display-spaces activate {}", hex(&packet))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn captured_registration_saves_cohorts_before_dispatch_and_reuses_identity() {
        let root = std::env::temp_dir().join(format!("lumen-display-runtime-test-{}",uuid::Uuid::new_v4()));
        let mut policy = Policy::open(root.join("spaces.json")).unwrap();
        let outputs = vec![json!({"id":1_152_921_504_606_846_976_u64,"name":"A","active":true,"focused":false,
            "rect":{"x":0,"y":0,"width":800,"height":600}}),
            json!({"id":1_152_921_504_606_846_977_u64,"name":"B","active":true,"focused":true,
            "rect":{"x":800,"y":0,"width":800,"height":600}})];
        let workspaces = vec![json!({"id":10,"name":"first","output":"A","visible":true}),
            json!({"id":11,"name":"second","output":"A","visible":false}),
            json!({"id":12,"name":"other","output":"B","visible":true})];
        policy.reconcile(&workspaces,&outputs).unwrap();
        let native = RefCell::new(json!({"version":1,"session":"synthetic","generation":"0",
            "available":false,"separateDisplays":true,"coupled":false,"primary":null}));
        let commands = RefCell::new(Vec::<Value>::new());
        let state_path = policy.path.clone();
        let mut query = |args: &[&str]| -> Result<Value> {
            if args == ["-t","get_outputs"] { return Ok(json!(outputs)); }
            if args == ["-t","get_tree"] { return Ok(json!({"display_spaces":*native.borrow()})); }
            let command = args.first().ok_or("Missing command")?;
            let payload = command.strip_prefix("lumen_window display-spaces configure ").ok_or("Unexpected command")?;
            let bytes = (0..payload.len()).step_by(2).map(|index|u8::from_str_radix(&payload[index..index+2],16).unwrap()).collect::<Vec<_>>();
            let packet: Value = serde_json::from_slice(&bytes).unwrap();
            let saved: Value = serde_json::from_slice(&std::fs::read(&state_path).unwrap()).unwrap();
            assert!(saved["automatic_primary"].is_string());
            assert_eq!(saved["display_cohorts"].as_array().unwrap().len(), 2);
            assert_eq!(packet["expectedGeneration"], native.borrow()["generation"]);
            commands.borrow_mut().push(packet.clone());
            let mut state = native.borrow_mut();
            let next = state["generation"].as_str().unwrap().parse::<u64>().unwrap()+1;
            state["generation"] = json!(next.to_string());
            state["available"] = json!(true);
            state["separateDisplays"] = packet["separateDisplays"].clone();
            state["coupled"] = json!(packet["separateDisplays"] == false);
            state["primary"] = packet["primary"].clone();
            Ok(json!([{"success":true}]))
        };
        let desired = json!({"separateDisplays":false});
        let first = synchronize(&mut policy,&mut query,Some(desired.clone())).unwrap();
        assert_eq!(first["applied"],true);
        assert_eq!(commands.borrow().len(),1);
        assert_eq!(commands.borrow()[0]["outputs"][0]["id"],"1152921504606846976");
        let before=std::fs::read(&policy.path).unwrap();
        synchronize(&mut policy,&mut query,Some(desired)).unwrap();
        assert_eq!(commands.borrow().len(),1);
        assert_eq!(std::fs::read(&policy.path).unwrap(),before);
        policy.release();
        let _=std::fs::remove_dir_all(root);
    }

    #[test]
    fn rejected_registration_keeps_durable_ids_for_retry() {
        let root=std::env::temp_dir().join(format!("lumen-display-retry-test-{}",uuid::Uuid::new_v4()));
        let mut policy=Policy::open(root.join("spaces.json")).unwrap();
        let outputs=vec![json!({"id":1,"name":"A","active":true,"rect":{"x":0,"y":0,"width":800,"height":600}})];
        let workspaces=vec![json!({"id":10,"name":"first","output":"A","visible":true})];
        policy.reconcile(&workspaces,&outputs).unwrap();
        let native=RefCell::new(json!({"version":1,"session":"synthetic","generation":"0",
            "available":false,"separateDisplays":true,"coupled":false,"primary":null}));
        let reject=RefCell::new(true);
        let mut query=|args:&[&str]|->Result<Value>{
            if args==["-t","get_outputs"] { return Ok(json!(outputs)); }
            if args==["-t","get_tree"] { return Ok(json!({"display_spaces":*native.borrow()})); }
            if *reject.borrow() { return Ok(json!([{"success":false}])); }
            let mut state=native.borrow_mut();state["generation"]=json!("1");state["available"]=json!(true);
            state["separateDisplays"]=json!(false);state["coupled"]=json!(true);state["primary"]=json!("A");
            Ok(json!([{"success":true}]))
        };
        assert!(synchronize(&mut policy,&mut query,Some(json!({"separateDisplays":false}))).is_err());
        let durable=policy.state["display_cohorts"].clone();
        assert!(!durable.as_array().unwrap().is_empty());
        *reject.borrow_mut()=false;
        assert_eq!(synchronize(&mut policy,&mut query,Some(json!({"separateDisplays":false}))).unwrap()["applied"],true);
        assert_eq!(policy.state["display_cohorts"],durable);
        policy.release();let _=std::fs::remove_dir_all(root);
    }

    #[test]
    fn preferred_connector_is_not_a_fallback_when_output_gains_metadata() {
        let native=json!({"available":true,"separateDisplays":true,"primary":"B","coupled":false});
        let desired=json!({"separateDisplays":true});
        let selected=status(&native,&desired,"B","connector:B","display:[\"Maker\",\"Panel\",\"123\"]").unwrap();
        assert_eq!(selected["fallback"],false);
        let missing=status(&native,&desired,"B","connector:A","display:[\"Maker\",\"Panel\",\"123\"]").unwrap();
        assert_eq!(missing["fallback"],true);
    }

    #[test]
    fn activation_requires_exact_captured_member_and_serializes_decimal_id() {
        let root=std::env::temp_dir().join(format!("lumen-display-activation-test-{}",uuid::Uuid::new_v4()));
        let mut policy=Policy::open(root.join("spaces.json")).unwrap();
        policy.state["display_separate"]=json!(false);
        policy.state["display_cohorts"]=json!([{"id":"one","members":{"connector:A":"logical-a"}}]);
        let native=RefCell::new(json!({"coupled":true,"available":true,"session":"synthetic","generation":"18446744073709551614"}));
        let mut query=|args:&[&str]|->Result<Value>{
            if args==["-t","get_tree"] { Ok(json!({"display_spaces":*native.borrow()})) }
            else { Err("unexpected query".into()) }
        };
        let desktop=json!({"logicalId":"logical-a","outputId":"connector:A","output":"A","name":"first","id":1152921504606846976_i64});
        let command=activation(&policy,&desktop,&mut query).unwrap().unwrap();
        let payload=command.strip_prefix("lumen_window display-spaces activate ").unwrap();
        let decoded=(0..payload.len()).step_by(2).map(|i|u8::from_str_radix(&payload[i..i+2],16).unwrap()).collect::<Vec<_>>();
        let packet:Value=serde_json::from_slice(&decoded).unwrap();
        assert_eq!(packet["expectedGeneration"],"18446744073709551614");
        assert_eq!(packet["session"],"synthetic");
        assert_eq!(packet["cohort"],"one");
        assert_eq!(packet["workspaceId"],"1152921504606846976");
        native.borrow_mut()["available"]=json!(false);
        assert!(activation(&policy,&desktop,&mut query).is_err());
        native.borrow_mut()["available"]=json!(true);
        let mut wrong=desktop.clone();wrong["logicalId"]=json!("other");
        assert!(activation(&policy,&wrong,&mut query).is_err());
        native.borrow_mut()["coupled"]=json!(false);
        assert!(activation(&policy,&desktop,&mut query).unwrap().is_none());
        policy.release();let _=std::fs::remove_dir_all(root);
    }
}
