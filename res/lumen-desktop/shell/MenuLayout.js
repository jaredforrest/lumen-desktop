function fit(widths, available, overflowWidth, needsOverflow) {
    const budget=Math.max(0,Number(available)||0);
    const values=widths.map(w=>Math.max(0,Number(w)||0));
    if(!needsOverflow && values.reduce((a,b)=>a+b,0)<=budget)return values.length;
    let used=0,count=0;
    for(const width of values){if(used+width>Math.max(0,budget-overflowWidth))break;used+=width;++count;}
    return count;
}
