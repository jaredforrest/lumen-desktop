// Canonical decimal strings retain all unsigned 64-bit native identity bits.
function component(value){
    if(typeof value==="number")return Number.isSafeInteger(value) && value>=0?String(value):null;
    if(typeof value!=="string")return null;
    const match=/^(0|[1-9][0-9]{0,19})$/.exec(value);
    if(!match || match[0]!==value)return null;
    if(value.length===20 && value>"18446744073709551615")return null;
    return value;
}
function normalize(value){
    if(!value || typeof value!=="object" || Object.keys(value).length!==2 || !("device" in value) || !("inode" in value))return null;
    const device=component(value.device),inode=component(value.inode);
    return device!==null && inode!==null?{device:device,inode:inode}:null;
}
function equal(left,right){const a=normalize(left),b=normalize(right);return !!a && !!b && a.device===b.device && a.inode===b.inode;}
if(typeof module!=="undefined")module.exports={component,normalize,equal};
