#!/usr/bin/env node
// Re-export only grid-unit metadata, preserving the current authored shapes,
// palettes and random variants. The HTML editor is the scale-policy source.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '..');
const html = fs.readFileSync(path.join(root, 'Cube/AssetShowcase.html'), 'utf8');
const policy = html.match(/<script id="asset-placement-scale">([\s\S]*?)<\/script>/);
assert(policy, 'Missing editor placement scale policy');
const context = {window:{}};
vm.runInNewContext(policy[1], context);
const scale = context.window.AssetPlacementScale;
const dir = path.join(root, 'Cube/Assets');
const check = process.argv.includes('--check');
let trees = 0, others = 0, changed = 0;
for (const name of fs.readdirSync(dir).filter(n => n.endsWith('.cubes')).sort()) {
  const file = path.join(dir, name), original = fs.readFileSync(file);
  assert.equal(original.toString('ascii',0,4), 'CUBE', name);
  assert.equal(original[4], 1, name);
  assert.equal(original[7], 8, name);
  const preset = name.slice(0,-6);
  const output = Buffer.from(original);
  output.writeFloatLE(scale.gridUnit(preset),12);
  if (scale.trees.includes(preset)) trees++; else others++;
  if (!output.equals(original)) {
    assert(!check, 'Stale asset scale: '+name);
    // Reject unexpected units before changing any exported file.
    assert([Math.fround(0.2),Math.fround(0.2/6)].includes(original.readFloatLE(12)), name);
    fs.writeFileSync(file, output);
    changed++;
  }
}
assert.equal(trees, scale.trees.length, 'Missing tree export');
console.log(`${trees} trees at c1; ${others} other assets at R1/2; ${changed} scale headers re-exported.`);
