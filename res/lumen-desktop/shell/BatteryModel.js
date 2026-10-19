function unavailable() {
    return {available:false,present:false,percentage:null,state:"unavailable",low:false,secondsRemaining:null};
}
function normalize(raw) {
    if(!raw || raw.available!==true)return unavailable();
    if(typeof raw.present!=="boolean")return unavailable();
    if(raw.present!==true)return Object.assign(unavailable(),{available:true,state:"not-present"});
    const states=["unknown","charging","discharging","empty","charged","pending-charge","pending-discharge"];
    if(typeof raw.percentage!=="number" || !Number.isFinite(raw.percentage) || raw.percentage<0 || raw.percentage>100 || !states.includes(raw.state))return unavailable();
    const time=raw.secondsRemaining;
    if(time!==null && (typeof time!=="number" || !Number.isFinite(time) || time<=0))return unavailable();
    return {available:true,present:true,percentage:raw.percentage,state:raw.state,
        low:["discharging","empty","pending-discharge"].includes(raw.state) && raw.percentage<=20,
        secondsRemaining:["charging","discharging"].includes(raw.state)?time:null};
}
function description(raw) {
    const power=normalize(raw);
    if(!power.available)return "Battery status unavailable";
    if(!power.present)return "No battery";
    const labels={unknown:"Status unknown",charging:"Charging",discharging:"On battery",empty:"Empty",charged:"Fully charged","pending-charge":"Not charging","pending-discharge":"On battery"};
    return Math.round(power.percentage)+"% · "+(power.low?"Low battery · ":"")+labels[power.state];
}
function estimate(raw) {
    const power=normalize(raw);
    if(!power.present || power.secondsRemaining===null)return "";
    const minutes=Math.ceil(power.secondsRemaining/60),hours=Math.floor(minutes/60),remainder=minutes%60;
    const value=hours?hours+" hr"+(remainder?" "+remainder+" min":""):minutes+" min";
    return "About "+value+(power.state==="charging"?" until full":" remaining");
}
if(typeof module!=="undefined")module.exports={unavailable,normalize,description,estimate};
