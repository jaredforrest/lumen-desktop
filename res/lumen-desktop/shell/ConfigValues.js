// Compare decoded configuration values without replacing an unchanged snapshot.
// Object field order is irrelevant; array order and actual value changes matter.
function equivalent(a,b) {
    if(a===b)return true;
    if(a===null || b===null || typeof a!=="object" || typeof b!=="object" || Array.isArray(a)!==Array.isArray(b))return false;
    const keys=Object.keys(a);
    if(keys.length!==Object.keys(b).length)return false;
    return keys.every(key=>Object.prototype.hasOwnProperty.call(b,key) && equivalent(a[key],b[key]));
}
