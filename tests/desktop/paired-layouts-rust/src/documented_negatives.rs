//! Finite negative cases required by the documented native paired contract.
use super::*;

const PAIR_ERROR: &str = "The selected windows cannot perform this paired layout action";
fn save(fixture: &Fixture<'_>, name: &str, value: &Value) -> Result<()> {
    fs::write(
        fixture.options.output.join(name),
        serde_json::to_vec_pretty(value)?,
    )?;
    Ok(())
}
fn pair_state(fixture: &Fixture<'_>, first: i64, second: i64) -> Result<Value> {
    Ok(
        json!({"first":fixture.required(first)?,"second":fixture.required(second)?,"parent":fixture.required_parent(first)?}),
    )
}
pub fn ratios(fixture: &Fixture<'_>, first: i64, second: i64) -> Result<()> {
    let mut receipts = Vec::new();
    for (index, value) in [
        "nan",
        "inf",
        "-inf",
        "0",
        "1",
        "-0.25",
        "1.25",
        "not-a-number",
        "0.5tail",
    ]
    .iter()
    .enumerate()
    {
        let before = pair_state(fixture, first, second)?;
        let command = fixture.rejected(
            &format!("lumen_pair ratio {first} {value}"),
            &format!("invalid-ratio-command-{index}.json"),
            PAIR_ERROR,
        )?;
        let after = pair_state(fixture, first, second)?;
        receipts.push(json!({"value":value,"command":command,"before":before,"after":after}));
        require(
            before == after,
            format!("Invalid ratio {value} changed native paired state"),
        )?;
    }
    save(fixture, "invalid-ratios.json", &json!(receipts))
}
pub fn survivor(fixture: &Fixture<'_>, window: i64) -> Result<()> {
    let before = fixture.required(window)?;
    require(
        before["lumen_pair"]["survivor"] == true && before["fullscreen_mode"] == 1,
        "Survivor negative subject is not a full-screen survivor",
    )?;
    let command = fixture.rejected(
        &format!("[con_id={window}] lumen_window minimize"),
        "survivor-minimize-command.json",
        "Window action is unavailable",
    )?;
    let after = fixture.required(window)?;
    save(
        fixture,
        "survivor-minimize-rejection.json",
        &json!({"before":before,"after":after,"command":command}),
    )?;
    require(
        before == after,
        "Rejected survivor minimize changed native state",
    )
}
pub fn no_focus(fixture: &mut Fixture<'_>, source: i64) -> Result<()> {
    fixture.yes(&format!("[con_id={source}] focus"))?;
    let before = fixture.required(source)?;
    require(
        before["focused"] == true,
        "No-focus test source is not actually focused",
    )?;
    let workspace = fixture.workspace(source)?;
    fixture.launch_foot("pair-no-focus")?;
    let newcomer = id(&fixture
        .app("pair-no-focus")?
        .ok_or("No-focus window missing")?)?;
    let after = fixture.required(source)?;
    let mapped = fixture.required(newcomer)?;
    save(
        fixture,
        "unrelated-no-focus.json",
        &json!({"sourceBefore":before,"sourceAfter":after,"newcomer":mapped,"sourceWorkspace":workspace,"newWorkspace":fixture.workspace(newcomer)?}),
    )?;
    require(
        after["focused"] == true
            && mapped["focused"] == false
            && fixture.workspace(newcomer)? != workspace
            && fixture.required_parent(source)?["nodes"]
                .as_array()
                .is_some_and(|rows| rows.len() == 2)
            && before["lumen_pair"] == after["lumen_pair"],
        "Unrelated no-focus window stole focus or entered the pair",
    )?;
    fixture.yes(&format!("[con_id={newcomer}] kill"))
}
fn relationship_window(fixture: &Fixture<'_>, title: &str, pid: u32) -> Result<Option<Value>> {
    Ok(find_node(&fixture.tree()?, &|node| {
        node["name"] == title && node["pid"] == pid
    }))
}
fn relationship_subjects(
    fixture: &Fixture<'_>,
    first: i64,
    second: i64,
    dialog: i64,
) -> Result<Value> {
    Ok(
        json!({"source":fixture.required(first)?,"other":fixture.required(second)?,"dialog":fixture.required(dialog)?}),
    )
}
pub fn relationships(fixture: &mut Fixture<'_>, anchor: i64) -> Result<()> {
    let client = fixture.children.launch(
        Path::new(env!("LUMEN_PAIR_RELATIONSHIP_CLIENT")),
        &[],
        &fixture.values,
        &fixture
            .options
            .output
            .join("paired-relationships-client.log"),
        true,
    )?;
    let pid = fixture.children.0[client].id();
    let old_owner = id(&wait("relationship old owner", || {
        relationship_window(fixture, "Relationship 0", pid)
    })?)?;
    let new_owner = id(&wait("relationship new owner", || {
        relationship_window(fixture, "Relationship 1", pid)
    })?)?;
    let initial_dialog = id(&wait("relationship initial dialog", || {
        relationship_window(fixture, "Relationship 2", pid)
    })?)?;
    require(
        fixture.required(initial_dialog)?["lumen_relationships"]["parent"] == old_owner,
        "Initial actual protocol owner missing",
    )?;
    fixture.client(client, "close-panel")?;
    wait("close initial relationship dialog", || {
        Ok(fixture.find(initial_dialog)?.is_none().then_some(()))
    })?;
    fixture.yes(&format!("lumen_pair create {anchor} {old_owner}"))?;
    let pair_workspace = fixture.workspace(anchor)?;
    fixture.client(client, "reopen-panel")?;
    let dialog = id(&wait("fresh tracked paired dialog", || {
        let Some(node) = relationship_window(fixture, "Relationship 2", pid)? else {
            return Ok(None);
        };
        let cid = id(&node)?;
        Ok((cid != initial_dialog
            && node["lumen_relationships"]["parent"] == old_owner
            && fixture.workspace(cid)? == pair_workspace)
            .then_some(node))
    })?)?;
    let original = relationship_subjects(fixture, old_owner, new_owner, dialog)?;
    fixture.client(client, "reparent")?;
    wait(
        "actual owner changes from paired old owner to new owner",
        || {
            let node = fixture.required(dialog)?;
            Ok((node["lumen_relationships"]["parent"] == new_owner
                && fixture.workspace(dialog)? == fixture.workspace(new_owner)?
                && fixture.workspace(dialog)? != pair_workspace)
                .then_some(()))
        },
    )?;
    let reparented = relationship_subjects(fixture, old_owner, new_owner, dialog)?;
    fixture.yes(&format!(
        "[con_id={old_owner}] move container to workspace paired-old-owner"
    ))?;
    require(
        fixture.workspace(dialog)? == fixture.workspace(new_owner)?
            && fixture.workspace(dialog)? != fixture.workspace(old_owner)?,
        "Dialog still followed retired protocol owner",
    )?;
    fixture.yes(&format!(
        "[con_id={new_owner}] move container to workspace paired-new-owner"
    ))?;
    wait("reparented dialog follows actual new owner", || {
        Ok((fixture.workspace(dialog)? == "paired-new-owner").then_some(()))
    })?;
    save(
        fixture,
        "actual-owner-reparent.json",
        &json!({"oldOwner":old_owner,"newOwner":new_owner,"dialog":dialog,"before":original,"reparented":reparented,"afterOldAndNewMoves":relationship_subjects(fixture,old_owner,new_owner,dialog)?}),
    )?;
    fixture.yes(&format!("lumen_pair leave {anchor}"))?;
    fixture.client(client, "unparent")?;
    wait("modal subject independent", || {
        let node = fixture.required(dialog)?;
        Ok((node["lumen_relationships"]["parent"].is_null()
            && node["lumen_pair"]["eligible"] == true)
            .then_some(()))
    })?;
    fixture.client(client, "modal")?;
    wait("actual parentless modal subject excluded", || {
        let node = fixture.required(dialog)?;
        Ok((node["lumen_relationships"]["modal"] == true
            && node["lumen_relationships"]["parent"].is_null()
            && node["lumen_pair"]["eligible"] == false)
            .then_some(()))
    })?;
    let before = json!({"anchor":fixture.required(anchor)?,"modal":fixture.required(dialog)?});
    let command = fixture.rejected(
        &format!("lumen_pair create {anchor} {dialog}"),
        "modal-primary-command.json",
        PAIR_ERROR,
    )?;
    let after = json!({"anchor":fixture.required(anchor)?,"modal":fixture.required(dialog)?});
    require(
        before == after,
        "Modal primary rejection changed native state",
    )?;
    fixture.client(client, "unmodal")?;
    wait(
        "same subject becomes eligible after protocol modal removal",
        || {
            let node = fixture.required(dialog)?;
            Ok((node["lumen_relationships"]["modal"] == false
                && node["lumen_pair"]["eligible"] == true)
                .then_some(()))
        },
    )?;
    fixture.yes(&format!("lumen_pair create {anchor} {dialog}"))?;
    require(
        fixture.required(anchor)?["lumen_pair"]["partner"] == dialog,
        "Unmodal same-window positive did not pair exact subject",
    )?;
    save(
        fixture,
        "modal-primary-rejection.json",
        &json!({"before":before,"after":after,"command":command,"unmodalPositive":fixture.required(dialog)?}),
    )?;
    fixture.yes(&format!("lumen_pair leave {dialog}"))?;
    fixture.yes(&format!("lumen_pair leave {anchor}"))?;
    fixture.client(client, "quit")?;
    fixture.children.wait_exit(client)?;
    wait("relationship client windows reaped", || {
        Ok(relationship_window(fixture, "Relationship 0", pid)?
            .is_none()
            .then_some(()))
    })?;
    Ok(())
}
