.pragma library
function status(row) {
    return ({broken:"broken",'chain-limit':"unresolved",'permission-denied':"permission denied",unavailable:"unavailable"})[row.linkStatus] || "";
}
function label(row) {
    const value=status(row);
    return row.name+(row.link && value?" ("+value+" link)":"");
}
function markedLabel(row) {
    const value=badge(row);
    return (value?value+" ":"")+label(row);
}
function badge(row) {
    return row.link?(status(row)?"↗ !":"↗"):"";
}
function accessible(row) {
    return label(row)+(row.link?", symbolic link":"");
}
