#!/usr/bin/env node
// Export only the generator's catalogue labels/IDs, never preview or camera data.
const fs=require('fs'),path=require('path');
const root=path.resolve(__dirname,'..'),dir=path.join(root,'Cube');
const html=fs.readFileSync(path.join(dir,'AssetShowcase.html'),'utf8');
const available=new Set(fs.readdirSync(path.join(dir,'Assets')).filter(n=>n.endsWith('.cubes')));
const groups=[],used=new Set();
for(const match of html.matchAll(/<optgroup label="([^"]+)">([\s\S]*?)<\/optgroup>/g)){
 const assets=[...match[2].matchAll(/<option value="([^"]+)">/g)].map(m=>m[1]+'.cubes').filter(n=>available.has(n));
 if(assets.length){groups.push({name:match[1].replaceAll('&amp;','&'),assets});assets.forEach(n=>used.add(n));}
}
const remaining=[...available].filter(n=>!used.has(n)).sort();
if(remaining.length)groups.push({name:'Other assets',assets:remaining});
const output=JSON.stringify({version:1,groups},null,2)+'\n',file=path.join(dir,'asset-groups.json');
if(process.argv.includes('--check')){if(fs.readFileSync(file,'utf8')!==output)throw Error('stale asset groups');}
else fs.writeFileSync(file,output);
console.log(groups.map(g=>`${g.name}: ${g.assets.length}`).join('\n'));
