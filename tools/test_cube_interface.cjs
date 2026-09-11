#!/usr/bin/env node
// Reference layouts and visible cells from the actual custom editor, no WebGL.
const fs=require('fs'),path=require('path'),vm=require('vm');
const root=path.resolve(__dirname,'..'),output=process.argv[2];
const html=fs.readFileSync(path.join(root,'Cube/AssetShowcase.html'),'utf8');
const start=html.lastIndexOf('(() => {',html.indexOf('const GLYPHS=Object.freeze(['));
const end=html.indexOf('\n</script>',start);
const context={window:{Cubes:{SubCubes:{requireTier:id=>({id,side:Number(String(id).replace('c',''))})}}}};
vm.createContext(context);vm.runInContext(html.slice(start,end),context);
const core=context.window.CUBE_UI_CORE;
const metadata=[];
for(const name of ['confirm','info','slider']){
 const d=JSON.parse(fs.readFileSync(path.join(root,`Cube/CubeInterface/${name}.json`),'utf8'));
 const model=core.build('custom',{menu:d.menu,state:d.state,tierId:d.tierId,themeId:d.themeId});
 const colors=Array(model.width*model.height).fill(core.themes[d.themeId].edge),z=Array(colors.length).fill(-1);
 for(const record of model.records){
  const x=record.gx/model.side+Math.floor(model.width/2),y=model.height-1-record.gy/model.side,i=y*model.width+x;
  if(record.gz>=z[i]){z[i]=record.gz;colors[i]=record.color;}
 }
 const bytes=Buffer.alloc(colors.length*4);colors.forEach((c,i)=>bytes.writeUInt32LE(c,i*4));
 fs.writeFileSync(path.join(output,`${name}.pixels`),bytes);
 metadata.push({name,width:model.width,height:model.height,widgets:model.widgets.map(w=>({rect:[w.x,w.y,w.width,w.height],action:w.action,disabled:w.disabled}))});
}
fs.writeFileSync(path.join(output,'reference.json'),JSON.stringify(metadata));
