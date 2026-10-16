import QtQuick
Canvas {
    id: root
    property string name
    property color ink:Theme.colors.text
    width:22;height:22
    onWidthChanged:requestPaint()
    onHeightChanged:requestPaint()
    onNameChanged:requestPaint()
    onInkChanged:requestPaint()
    onPaint: {
        const c=getContext("2d");c.reset();c.scale(width/24,height/24);
        c.strokeStyle=ink;c.fillStyle=ink;c.lineWidth=1.7;c.lineCap="round";c.lineJoin="round";
        function line(points){c.beginPath();c.moveTo(points[0][0],points[0][1]);for(let i=1;i<points.length;i++)c.lineTo(points[i][0],points[i][1]);c.stroke();}
        function arc(x,y,r,a,b){c.beginPath();c.arc(x,y,r,a,b);c.stroke();}
        if(name==="wifi") {
            for(const r of [5,9,13])arc(12,21,r,Math.PI*1.25,Math.PI*1.75);
            c.beginPath();c.arc(12,20,1.1,0,Math.PI*2);c.fill();
        } else if(name==="bluetooth") {
            line([[12,2],[12,22],[18,16],[6,6]]);line([[12,2],[18,8],[6,18]]);
        } else if(name==="airdrop") {
            for(const r of [4,7,10])arc(12,11,r,Math.PI*.2,Math.PI*2.8);
            line([[8,22],[12,16],[16,22],[8,22]]);
        } else if(name==="moon") {
            c.beginPath();c.moveTo(16,3);c.bezierCurveTo(0,0,0,23,16,21);c.bezierCurveTo(8,18,8,7,16,3);c.stroke();
        } else if(name==="mirror") {
            c.strokeRect(3,3,14,12);c.strokeRect(8,9,13,12);
        } else if(name==="screen") {
            c.strokeRect(3,4,18,13);line([[8,21],[16,21]]);line([[12,17],[12,21]]);
        } else if(name==="capture") {
            line([[3,8],[3,3],[8,3]]);line([[16,3],[21,3],[21,8]]);line([[21,16],[21,21],[16,21]]);line([[8,21],[3,21],[3,16]]);c.strokeRect(7,7,10,10);
        } else if(name==="speaker" || name==="speaker-low" || name==="muted") {
            line([[3,9],[7,9],[12,5],[12,19],[7,15],[3,15],[3,9]]);if(name==="speaker") {arc(12,12,5,-.8,.8);arc(12,12,9,-.8,.8);}
            else if(name==="muted") {line([[16,9],[22,15]]);line([[22,9],[16,15]]);}
        } else if(name==="play" || name==="next" || name==="previous") {
            if(name==="previous"){c.translate(24,0);c.scale(-1,1);}
            c.beginPath();c.moveTo(6,5);c.lineTo(17,12);c.lineTo(6,19);c.closePath();c.fill();
            if(name!=="play")c.fillRect(18,5,2,14);
        } else if(name==="sun") {
            arc(12,12,4,0,Math.PI*2);
            for(let i=0;i<8;i++){const a=i*Math.PI/4;line([[12+7*Math.cos(a),12+7*Math.sin(a)],[12+10*Math.cos(a),12+10*Math.sin(a)]]);}
        } else if(name==="search") {
            arc(10,10,6,0,Math.PI*2);line([[15,15],[21,21]]);
        } else if(name==="home") {
            line([[3,11],[12,3],[21,11]]);line([[6,10],[6,21],[10,21],[10,15],[14,15],[14,21],[18,21],[18,10]]);
        } else if(name==="folder") {
            line([[3,6],[9,6],[11,9],[21,9],[21,20],[3,20],[3,6]]);
        } else if(name==="download") {
            line([[12,3],[12,16],[7,11]]);line([[12,16],[17,11]]);line([[4,16],[4,21],[20,21],[20,16]]);
        } else if(name==="document") {
            line([[5,2],[14,2],[19,7],[19,22],[5,22],[5,2]]);line([[14,2],[14,7],[19,7]]);line([[8,12],[16,12]]);line([[8,16],[16,16]]);
        } else if(name==="clock") {
            arc(12,12,9,0,Math.PI*2);line([[12,6],[12,12],[16,14]]);
        } else if(name==="grid") {
            for(let x=4;x<=14;x+=10)for(let y=4;y<=14;y+=10)c.strokeRect(x,y,6,6);
        } else if(name==="list") {
            for(let y=6;y<=18;y+=6){line([[4,y],[5,y]]);line([[9,y],[20,y]]);}
        } else if(name==="more") {
            for(let y=5;y<=19;y+=7){c.beginPath();c.arc(12,y,1.3,0,Math.PI*2);c.fill();}
        } else if(name==="chevron-left" || name==="chevron-right") {
            if(name==="chevron-left")line([[15,5],[8,12],[15,19]]);else line([[9,5],[16,12],[9,19]]);
        } else if(name==="minus" || name==="plus") {
            line([[5,12],[19,12]]);if(name==="plus")line([[12,5],[12,19]]);
        } else if(name==="close") {
            line([[6,6],[18,18]]);line([[18,6],[6,18]]);
        } else if(name==="copy") {
            c.strokeRect(8,7,12,14);line([[5,17],[3,17],[3,2],[16,2],[16,4]]);
        } else if(name==="save") {
            line([[12,2],[12,15],[8,11]]);line([[12,15],[16,11]]);line([[4,14],[4,21],[20,21],[20,14]]);
        } else if(name==="undo" || name==="redo") {
            if(name==="redo"){c.translate(24,0);c.scale(-1,1);}
            line([[8,4],[3,9],[8,14]]);c.beginPath();c.moveTo(3,9);c.lineTo(14,9);c.bezierCurveTo(23,9,23,21,13,21);c.stroke();
        } else if(name==="lock") {
            c.strokeRect(5,10,14,11);c.beginPath();c.arc(12,10,5,Math.PI,Math.PI*2);c.stroke();line([[12,14],[12,17]]);
        } else if(name==="info") {
            arc(12,12,9,0,Math.PI*2);line([[12,11],[12,17]]);c.beginPath();c.arc(12,7,1,0,Math.PI*2);c.fill();
        } else if(name==="gear") {
            arc(12,12,7,0,Math.PI*2);arc(12,12,3,0,Math.PI*2);
            for(let i=0;i<8;i++){const a=i*Math.PI/4;line([[12+7*Math.cos(a),12+7*Math.sin(a)],[12+10*Math.cos(a),12+10*Math.sin(a)]]);}
        } else if(name==="person") {
            arc(12,7,4,0,Math.PI*2);c.beginPath();c.moveTo(4,22);c.bezierCurveTo(4,11,20,11,20,22);c.stroke();
        } else if(name==="network") {
            arc(12,12,9,0,Math.PI*2);c.beginPath();c.ellipse(8,3,8,18);c.stroke();line([[3,12],[21,12]]);
        } else if(name==="bell") {
            c.beginPath();c.moveTo(5,17);c.lineTo(7,14);c.lineTo(7,8);c.bezierCurveTo(7,1,17,1,17,8);c.lineTo(17,14);c.lineTo(19,17);c.closePath();c.stroke();arc(12,19,2,0,Math.PI);
        } else if(name==="music") {
            line([[10,18],[10,5],[20,3],[20,16]]);arc(7,18,3,0,Math.PI*2);arc(17,16,3,0,Math.PI*2);
        } else {
            arc(12,12,9,0,Math.PI*2);c.beginPath();c.arc(12,12,6,-Math.PI/2,Math.PI/2);c.fill();
        }
    }
}
