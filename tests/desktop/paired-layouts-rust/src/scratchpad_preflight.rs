//! Refusal precedes pair teardown; ordinary raw scratchpad behavior remains usable.
use super::*;
fn reject(fixture:&Fixture<'_>,id:i64,label:&str)->Result<()> {
    let mut previous=None;
    let before=wait("settled native pair before raw scratchpad refusal",||{
        let tree=fixture.tree()?;let ready=previous.as_ref()==Some(&tree);previous=Some(tree.clone());Ok(ready.then_some(tree))
    })?;
    let reply=fixture.rejected(&format!("[con_id={id}] move scratchpad"),&format!("scratchpad-{label}-command.json"),"Paired layout cannot move to scratchpad")?;
    let after=fixture.tree()?;
    fs::write(fixture.options.output.join(format!("scratchpad-{label}.json")),serde_json::to_vec_pretty(&json!({"before":before,"after":after,"reply":reply}))?)?;
    require(before==after,format!("Raw {label} scratchpad refusal changed native state/focus/geometry"))?;
    require(fixture.query("get_version")?["native_paired_layouts"]==true,"Compositor did not survive paired scratchpad refusal")
}
pub fn run(fixture:&mut Fixture<'_>,first:i64,second:i64)->Result<()> {
    fixture.yes(&format!("lumen_pair create {first} {second}"))?;
    let parent=fixture.required_parent(first)?;
    require(parent["fullscreen_mode"]==1&&parent["pid"].is_null(),"Expected native non-view paired parent")?;
    reject(fixture,id(&parent)?,"parent")?;
    reject(fixture,first,"member")?;
    fixture.yes(&format!("lumen_pair leave {first}"))?;
    require(fixture.required(second)?["lumen_pair"]["survivor"]==true,"Expected retained full-screen survivor")?;
    reject(fixture,second,"survivor")?;
    fixture.yes(&format!("lumen_pair leave {second}"))?;
    // Raw scratchpad show retains native scratch membership. Use a separate
    // owned client so that later paired cases keep their original eligibility.
    fixture.launch_foot("pair-scratch-positive")?;
    let ordinary=id(&fixture.app("pair-scratch-positive")?.ok_or("Scratchpad positive client missing")?)?;
    fixture.yes(&format!("[con_id={ordinary}] floating enable, resize set 360 px 300 px, move absolute position 460 px 200 px"))?;
    fixture.yes(&format!("[con_id={ordinary}] focus"))?;
    let before=fixture.required(ordinary)?;
    let workspace=fixture.workspace(ordinary)?;
    let rectangle=fixture.box_rect(ordinary)?;
    require(before["lumen_pair"]["id"].is_null()&&before["lumen_pair"]["survivor"]==false,"Ordinary positive still paired")?;
    fixture.yes(&format!("[con_id={ordinary}] move scratchpad"))?;
    let hidden=fixture.required(ordinary)?;
    require(fixture.workspace(ordinary)?=="__i3_scratch"&&hidden["scratchpad_state"]!="none","Ordinary raw move did not enter scratchpad")?;
    fixture.yes(&format!("[con_id={ordinary}] scratchpad show"))?;
    let restored=fixture.required(ordinary)?;
    require(restored["pid"]==before["pid"]&&restored["app_id"]==before["app_id"]&&fixture.workspace(ordinary)?==workspace&&fixture.box_rect(ordinary)?==rectangle,"Ordinary raw scratchpad/show did not preserve identity/placement")?;
    fixture.yes(&format!("[con_id={ordinary}] kill"))?;
    wait("dedicated scratchpad positive client closed",||Ok(fixture.app("pair-scratch-positive")?.is_none().then_some(())))?;
    fs::write(fixture.options.output.join("scratchpad-preflight-result.json"),serde_json::to_vec_pretty(&json!({"passed":true,"checks":4,"effects":fixture.options.effects,"parent":parent,"ordinaryBefore":before,"ordinaryHidden":hidden,"ordinaryRestored":restored,"dedicatedClientClosed":true,"scope":"Exact parent/member/survivor CMD_INVALID with whole-tree and focus unchanged; dedicated ordinary raw scratchpad/show positive and owned client closure"}))?)?;
    println!("PASS paired raw scratchpad preflight and ordinary positive");Ok(())
}
