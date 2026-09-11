#!/usr/bin/env node
// Extract the custom editor's original MicroFont, palette and icon data only.
const fs=require('fs'),path=require('path'),vm=require('vm');
const root=path.resolve(__dirname,'..');
const html=fs.readFileSync(path.join(root,'Cube/AssetShowcase.html'),'utf8');
const start=html.lastIndexOf('(() => {',html.indexOf('const GLYPHS=Object.freeze(['));
const end=html.indexOf('\n</script>',start);
const context={window:{}};vm.createContext(context);vm.runInContext(html.slice(start,end),context);
const core=context.window.CUBE_UI_CORE;
let source='// Extracted from Cube/AssetShowcase.html by tools/export_interface_style.cjs.\n';
source+='// MicroFont 3.7.8, MIT License, Copyright (c) 2026 t4ce.\n';
source+='// Permission is hereby granted, free of charge, to any person obtaining a copy\n// of this software and associated documentation files (the "Software"), to deal\n// in the Software without restriction, including without limitation the rights\n// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell\n// copies of the Software, and to permit persons to whom the Software is\n// furnished to do so, subject to the following conditions:\n// The above copyright notice and this permission notice shall be included in all\n// copies or substantial portions of the Software.\n// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR\n// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,\n// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE\n// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER\n// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,\n// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.\n';
source+='#[rustfmt::skip]\npub const GLYPHS: [u64; 95] = [\n'+Array.from({length:95},(_,i)=>'0x'+core.glyphBits(i+32).toString(16)+'u64').join(',\n')+'\n];\n';
source+='#[rustfmt::skip]\npub const THEMES: [[u32; 12]; 3] = [\n';
for(const name of ['fern','glacier','ember']) source+='['+['edge','panel','header','ink','muted','accent','light','button','track','gold','danger','disabled'].map(k=>'0x'+core.themes[name][k].toString(16)).join(',')+'],\n';
source+='];\n#[rustfmt::skip]\npub const ICONS: [[u8; 7]; 10] = [\n';
for(const name of ['close','check','plus','minus','left','right','heart','menu','play','gear']) source+='['+core.icons[name].map(row=>'0b'+row).join(',')+'],\n';
source+='];\n';
const output=path.join(root,'src/interface_style.rs');
if(process.argv.includes('--check')){if(fs.readFileSync(output,'utf8')!==source)throw Error('stale interface style');}
else fs.writeFileSync(output,source);
