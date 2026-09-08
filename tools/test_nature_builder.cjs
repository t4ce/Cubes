// Exercise the actual browser serializer without WebGL or remote imports.
const fs = require('fs');
const vm = require('vm');
const assert = require('assert/strict');
const html=fs.readFileSync(require('path').join(__dirname,'../Cube/cube_tree_builder.html'),'utf8');
const functionText=html.slice(html.indexOf('function makeCubesBinary(){'),html.indexOf('\nfunction downloadCubes(){'));
const context={placed:[
  {gx:-2,gy:0,gz:0,tier:2,color:0xff0000,kind:'trunk'},
  {gx:0,gy:0,gz:0,tier:1,color:0x00ff00,kind:'leaf'},
],GRID_UNIT:0.2,GAP:0.01,MAX_CUBE_STEP:4,PART_IDS:new Map([['trunk',1],['leaf',3]]),
alert:()=>{},cellsFor:(x,y,z,n)=>Array.from({length:n*n*n},(_,i)=>[x+i%n,y+Math.floor(i/n)%n,z+Math.floor(i/(n*n))])};
vm.createContext(context); vm.runInContext(functionText,context);
const bytes=Buffer.from(context.makeCubesBinary());
assert.deepEqual([...bytes.subarray(0,12)],[67,85,66,69,1,0,1,8,2,0,2,4]);
assert.deepEqual([...bytes.subarray(24)],[254,0,0,2,0,1,0,0,0,0,0,1,1,3,0,0]);
context.placed[1].gx=-1; assert.equal(context.makeCubesBinary(),null);
context.placed[1].gx=0.5; assert.equal(context.makeCubesBinary(),null);
console.log('Nature builder: strict-grid header, records, overlap and integer checks passed');
