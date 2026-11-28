// NEPTUNE animated README identity; Node stdlib only; no runtime/website JavaScript.
// Generate deterministic GIF89a from original pixel geometry. No external assets.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
function buildGif(){
  const W=1280,H=300, F=16;
  const pal=[[10,17,23],[16,31,40],[41,71,83],[53,86,101],[65,110,122],[95,206,217],[219,233,236],[159,233,235],[115,169,182],[208,169,91],[225,236,239],[49,75,89],[0,0,0],[0,0,0],[0,0,0],[0,0,0]];
  const fontRows={
"A":"01110/10001/10001/11111/10001/10001/10001","B":"11110/10001/10001/11110/10001/10001/11110","C":"01111/10000/10000/10000/10000/10000/01111","D":"11110/10001/10001/10001/10001/10001/11110","E":"11111/10000/10000/11110/10000/10000/11111","F":"11111/10000/10000/11110/10000/10000/10000","G":"01111/10000/10000/10111/10001/10001/01111","H":"10001/10001/10001/11111/10001/10001/10001","I":"11111/00100/00100/00100/00100/00100/11111","J":"00111/00010/00010/00010/10010/10010/01100","K":"10001/10010/10100/11000/10100/10010/10001","L":"10000/10000/10000/10000/10000/10000/11111","M":"10001/11011/10101/10101/10001/10001/10001","N":"10001/11001/10101/10011/10001/10001/10001","O":"01110/10001/10001/10001/10001/10001/01110","P":"11110/10001/10001/11110/10000/10000/10000","Q":"01110/10001/10001/10001/10101/10010/01101","R":"11110/10001/10001/11110/10100/10010/10001","S":"01111/10000/10000/01110/00001/00001/11110","T":"11111/00100/00100/00100/00100/00100/00100","U":"10001/10001/10001/10001/10001/10001/01110","V":"10001/10001/10001/10001/10001/01010/00100","W":"10001/10001/10001/10101/10101/10101/01010","X":"10001/10001/01010/00100/01010/10001/10001","Y":"10001/10001/01010/00100/00100/00100/00100","Z":"11111/00001/00010/00100/01000/10000/11111","0":"01110/10011/10101/10101/11001/10001/01110","1":"00100/01100/00100/00100/00100/00100/01110","2":"01110/10001/00001/00010/00100/01000/11111","3":"11110/00001/00001/01110/00001/00001/11110","4":"00010/00110/01010/10010/11111/00010/00010","5":"11111/10000/10000/11110/00001/00001/11110","6":"01110/10000/10000/11110/10001/10001/01110","7":"11111/00001/00010/00100/01000/01000/01000","8":"01110/10001/10001/01110/10001/10001/01110","9":"01110/10001/10001/01111/00001/00001/01110",".":"00000/00000/00000/00000/00000/00100/00100","/":"00001/00001/00010/00100/01000/10000/10000","-":"00000/00000/00000/11111/00000/00000/00000"," ":"00000/00000/00000/00000/00000/00000/00000"
};
  const font={};for(const [k,v] of Object.entries(fontRows))font[k]=v.split("/");
  const base=new Uint8Array(W*H);
  function rect(dst,x,y,w,h,c){
    if(x<0||y<0||x+w>W||y+h>H||w<=0||h<=0)throw Error("BAD_RECT");
    for(let yy=y;yy<y+h;yy++)dst.fill(c,yy*W+x,yy*W+x+w);
  }
  function pixelText(dst,t,x,y,scale,c){
    for(const ch of t){
      const rows=font[ch];if(!rows)throw Error("BAD_GLYPH_"+ch);
      for(let j=0;j<7;j++)for(let i=0;i<5;i++)if(rows[j][i]==="1")rect(dst,x+i*scale,y+j*scale,scale,scale,c);
      x+=scale*6;
    }
  }
  // Fixed-color opaque 16-color geometry. No imported artwork or fonts.
  rect(base,16,16,1248,2,3);rect(base,16,283,1248,2,3);
  rect(base,16,16,2,268,3);rect(base,1262,16,2,268,3);
  rect(base,16,52,1248,2,2);rect(base,16,249,1248,2,2);
  for(let i=0;i<3;i++){rect(base,38+i*14,36,7,7,5);rect(base,1198+i*20,36,15,5,5);}
  rect(base,50,67,247,166,3);rect(base,52,69,243,162,1);
  for(const y of [101,125,149,173,197]){rect(base,62,y,10,4,4);rect(base,217,y,22,4,4);}
  rect(base,72,87,28,125,5);rect(base,189,87,28,125,5);
  const poly=[[100,87],[124,87],[189,177],[189,212],[166,212],[100,122]];
  for(let y=87;y<213;y++){
    const xx=[];for(let k=0;k<poly.length;k++){
      const a=poly[k],b=poly[(k+1)%poly.length];
      if((a[1]<=y&&b[1]>y)||(b[1]<=y&&a[1]>y))xx.push(a[0]+(y-a[1])*(b[0]-a[0])/(b[1]-a[1]));
    }
    xx.sort((a,b)=>a-b);for(let k=0;k+1<xx.length;k+=2){const left=Math.max(100,Math.ceil(xx[k])),right=Math.min(190,Math.floor(xx[k+1]));if(right>=left)rect(base,left,y,right-left+1,1,6);}
  }
  for(const [x,y] of [[245,89],[245,203],[57,88],[57,204]])rect(base,x,y,7,7,9);
  pixelText(base,"X64 / NATIVE COMPATIBILITY RUNTIME",354,63,2,8);
  const letters="NEPTUNE";
  for(let n=0;n<letters.length;n++)for(let row=0;row<7;row++)for(let col=0;col<5;col++)
    if(font[letters[n]][row][col]==="1")rect(base,354+n*120+col*18,83+row*18,14,14,(n===0||n===6)?5:6);
  rect(base,355,227,815,2,4);
  pixelText(base,"UNDERSTAND THE MACHINE. PROVE THE PORT.",354,257,2,6);
  for(let i=0;i<3;i++)rect(base,1058+i*17,261,9,9,9);
  function encodeLzw(src){
    const clear=16,end=17;let next=18,bits=5,dict=new Map;
    for(let i=0;i<16;i++)dict.set(String.fromCharCode(i),i);
    let buf=0,n=0;const out=[];
    function emit(code){buf|=code<<n;n+=bits;while(n>=8){out.push(buf&255);buf>>>=8;n-=8;}}
    emit(clear);let word=String.fromCharCode(src[0]);
    for(let i=1;i<src.length;i++){
      const letter=String.fromCharCode(src[i]),joined=word+letter;
      if(dict.has(joined)){word=joined;continue;}
      emit(dict.get(word));
      if(next<4096){dict.set(joined,next++);if(next>(1<<bits)&&bits<12)bits++;}
      else{
        emit(clear);dict=new Map;for(let k=0;k<16;k++)dict.set(String.fromCharCode(k),k);
        bits=5;next=18;
      }
      word=letter;
    }
    emit(dict.get(word));emit(end);if(n)out.push(buf&255);
    return out;
  }
  const out=[];
  function push(...a){out.push(...a);}
  function u16(x){push(x&255,(x>>>8)&255);}
  for(const c of "GIF89a")push(c.charCodeAt(0));
  u16(W);u16(H);push(0xF3,0,0);
  for(const p of pal)push(...p);
  for(const c of [0x21,0xFF,0x0B,...Array.from("NETSCAPE2.0").map(c=>c.charCodeAt(0)),0x03,0x01,0,0,0])push(c);
  for(let frame=0;frame<F;frame++){
    const pix=base.slice();
    if(frame!==0&&frame!==15){
      const led=Math.floor(frame/2)%3;
      for(let j=0;j<3;j++)rect(pix,850+12*j,36,10,5,j===led?7:3);
      rect(pix,354+((frame-1)%7)*120,83,14,14,7);
      rect(pix,355+(frame-1)*48,226,20,3,5);
      rect(pix,1058,261,43,9,3);
      rect(pix,1058+17*(Math.floor(frame/3)%3),261,9,9,9);
    }
    push(0x21,0xF9,0x04,0x04,16,0,0,0); // GCE delay 160 ms
    push(0x2C);u16(0);u16(0);u16(W);u16(H);push(0);
    push(4);const bytes=encodeLzw(pix);
    for(let i=0;i<bytes.length;i+=255){let c=bytes.slice(i,i+255);push(c.length);push(...c);}
    push(0);
  }
  push(0x3B);
  return Uint8Array.from(out);
}
const target=path.resolve(__dirname,'../docs/assets/neptune-logo-animated.gif');
const result=Buffer.from(buildGif());
const digest=crypto.createHash('sha256').update(result).digest('hex');
if(process.argv.length!==3||!['--check','--write'].includes(process.argv[2]))throw Error('USAGE: node tools/neptune-logo-animation.cjs --check|--write');
if(process.argv[2]==='--check'){
  if(!fs.existsSync(target)||!fs.readFileSync(target).equals(result))throw Error('GIF_BYTE_PARITY_FAIL '+digest);
  console.log('R63_GIF_PARITY=PASS frames=16 size='+result.length+' sha256='+digest);
}else{fs.mkdirSync(path.dirname(target),{recursive:true});fs.writeFileSync(target,result);console.log('R63_GIF_WRITTEN sha256='+digest);}
