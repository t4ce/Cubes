#!/usr/bin/env node
// Exercise the real shared loader and deterministic world model with re-exports.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {parse} = require('../Cube/material-palette.js');
const root = path.resolve(__dirname, '..');
const document = JSON.parse(fs.readFileSync(path.join(root, 'Cube/subcubes-materials.json')));
const palette = parse(document);
assert.deepEqual(palette.materials.map(m => m.id), ['red','orange','yellow','green','blue','violet']);
assert.equal(parse({...document, materials: [...document.materials].reverse()}).fingerprint, palette.fingerprint);
for (const mutate of [d => d.materials.pop(), d => d.materials[0].id = 'blue', d => d.colorSpace = 'linear',
  d => d.materials[0].rgb.r = NaN, d => d.materials[0].roughness = -1, d => d.materials[0].metallic = 2]) {
  const bad = structuredClone(document); mutate(bad); assert.throws(() => parse(bad));
}
const html = fs.readFileSync(path.join(root, 'Cube/WorldShowcase.html'), 'utf8');
function between(start, end) {const a = html.indexOf(start); return html.slice(a, html.indexOf(end, a));}
function worldModel(materials) {
  const context = {MATERIAL_PALETTE: materials};
  vm.createContext(context);
  vm.runInContext([
    between('const clamp=', 'function lookQuaternion'),
    between('function hexRGB', 'function cubeGeometryFromGLB'),
    between('const WORLD_THEMES', 'function geometryOverlapsBox'),
    'globalThis.result={WORLD_THEMES,PALETTES,RAINBOW,WORLDS,portalPalette};'
  ].join('\n'), context);
  return context.result;
}
const original = worldModel(palette);
const themeIds = ['blue','orange','violet','yellow','green','red'];
for (const [i,id] of themeIds.entries()) {
  assert.equal(original.WORLD_THEMES[i].name, palette.byId[id].name);
  assert.equal(original.PALETTES[i+1], palette.byId[id].hex);
  assert.equal(original.portalPalette([i+1])[0].hex, palette.byId[id].hex);
}
assert.equal(original.RAINBOW.length, 6);
assert.deepEqual(Array.from(original.portalPalette(['S']), m => m.hex), palette.materials.map(m => m.hex));
const edited = structuredClone(document);
edited.materials.find(m => m.id === 'blue').rgb = {r: 0.2, g: 0.3, b: 0.4};
const changed = worldModel(parse(edited));
assert.equal(changed.PALETTES[1], '#334d66');
assert.equal(changed.portalPalette([1])[0].hex, '#334d66');
assert.equal(changed.RAINBOW[4], '#334d66');
assert.equal(JSON.stringify(changed.WORLDS), JSON.stringify(original.WORLDS), 'palette edits must preserve world identities/topology');
console.log('Shared palette validation, stable IDs, six theme/portal bands, and re-export propagation passed.');
