function reveal(offset,viewport,content,start,length,margin) {
    const padding=margin===undefined?8:margin;
    let next=offset;
    if(start-padding<offset)next=start-padding;
    else if(start+length+padding>offset+viewport)next=length+padding*2>viewport?start-padding:start+length+padding-viewport;
    return Math.max(0,Math.min(Math.max(0,content-viewport),next));
}
