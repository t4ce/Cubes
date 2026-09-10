#!/usr/bin/env node
// Export the default v16 WorldShowcase generation as the runtime's compact
// strict-grid CUBES assets.  One grid cell represents one c4 (8 c1 units),
// so the 2048-c1 world fits the format's signed 256-cell address range.
const fs = require('fs');
const path = require('path');
const vm = require('vm');

const root = path.resolve(__dirname, '..');
const html = fs.readFileSync(path.join(root, 'Cube/WorldShowcase.html'), 'utf8');
function between(start, end) {
  const a = html.indexOf(start);
  const b = html.indexOf(end, a);
  if (a < 0 || b < 0) throw new Error(`cannot extract generator section: ${start}`);
  return html.slice(a, b);
}

// The showcase deliberately keeps its deterministic world model independent
// of its WebGL/UI layer. Evaluate only that model, then generate all defaults.
const model = [
  between('const clamp=', 'class CubeRenderer'),
  between('const WORLD_THEMES', 'function geometryOverlapsBox'),
  `
  for (const world of WORLDS) generatePlatforms(world, levelData[world.id - 1]);
  globalThis.__api = { V3, themeIndexAt, hashSeed, PALETTES, RAINBOW, themeFor };
  globalThis.__lvl27 = WORLDS.map((world) => ({
    world,
    data: levelData[world.id - 1],
    geometry: buildGeometry(world, levelData[world.id - 1]),
  }));
  `,
].join('\n');
const context = { console, globalThis: {} };
vm.createContext(context);
vm.runInContext(model, context, { filename: 'WorldShowcase-default-model.js' });

function rgba(hex) {
  const value = Number.parseInt(hex.slice(1), 16);
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255, 255];
}
function colorFor(entry, voxel) {
  const api = context.globalThis.__api;
  const { world, data } = entry;
  if (world.kind === 'special') {
    // Strict-grid assets cap at 16,384 records. The studio's one-c4-cell
    // rainbow would exceed that limit, so retain its palette as deterministic
    // 4×4×4 c4 colour fields, which can be represented as c4 records.
    const macro = [voxel.x, voxel.y, voxel.z].map(v => Math.floor((v + 1024) / 32));
    return api.RAINBOW[api.hashSeed(macro.join(',')) % api.RAINBOW.length];
  }
  const theme = voxel.theme ?? world.themes[api.themeIndexAt(
    world,
    new api.V3(voxel.x + 4, voxel.y + 4, voxel.z + 4),
    api.hashSeed(data.config.seed),
  )];
  return api.PALETTES[theme];
}

function compact(entry) {
  const occupied = new Map();
  for (const voxel of entry.geometry.primary.values()) {
    // Defaults have no c3 growth and no placed c1 controls. All source voxels
    // are full c4 cells, which are exactly representable by this export grid.
    if (voxel.size !== 8) throw new Error(`unexpected non-c4 voxel in world ${entry.world.id}`);
    const cell = [(voxel.x + 1024) / 8, (voxel.y + 1024) / 8, (voxel.z + 1024) / 8];
    if (!cell.every(Number.isInteger) || !cell.every(n => n >= 0 && n < 256)) throw new Error(`out-of-range c4 cell in world ${entry.world.id}`);
    // The runtime's portal-transition code uses parts 9/10.  The exported
    // c4 portal bridge is its compatible representation (the studio's c2
    // decorative ring cannot be expressed in this c4-grid format).
    const part = voxel.kind === 'portalPath' ? ((cell[0] + cell[1] + cell[2]) & 1 ? 9 : 10) : 0;
    occupied.set(cell.join(','), { cell, color: colorFor(entry, voxel), part });
  }
  const used = new Set();
  const records = [];
  const cells = [...occupied.values()].sort((a, b) => a.cell[0] - b.cell[0] || a.cell[1] - b.cell[1] || a.cell[2] - b.cell[2]);
  const key = (x, y, z) => `${x},${y},${z}`;
  for (const item of cells) {
    const [x, y, z] = item.cell;
    if (used.has(key(x, y, z))) continue;
    let tier = 1;
    for (const candidate of [4, 3, 2]) {
      let matches = true;
      for (let dx = 0; dx < candidate && matches; dx++) for (let dy = 0; dy < candidate && matches; dy++) for (let dz = 0; dz < candidate; dz++) {
        const other = occupied.get(key(x + dx, y + dy, z + dz));
        if (!other || used.has(key(x + dx, y + dy, z + dz)) || other.color !== item.color || other.part !== item.part) { matches = false; break; }
      }
      if (matches) { tier = candidate; break; }
    }
    for (let dx = 0; dx < tier; dx++) for (let dy = 0; dy < tier; dy++) for (let dz = 0; dz < tier; dz++) used.add(key(x + dx, y + dy, z + dz));
    records.push({ x: x - 128, y: y - 128, z: z - 128, tier, color: item.color, part: item.part });
  }
  if (used.size !== occupied.size) throw new Error(`incomplete compaction in world ${entry.world.id}`);
  return records;
}

function encode(entry) {
  const records = compact(entry);
  if (records.length > 16384) throw new Error(`world ${entry.world.id} has ${records.length} records (runtime limit is 16384)`);
  const palette = [];
  const paletteIndex = new Map();
  for (const record of records) if (!paletteIndex.has(record.color)) {
    paletteIndex.set(record.color, palette.length);
    palette.push(rgba(record.color));
  }
  const output = Buffer.alloc(16 + palette.length * 4 + records.length * 8);
  output.write('CUBE', 0, 'ascii'); output[4] = 1; output[5] = 0; output[6] = 1; output[7] = 8;
  output.writeUInt16LE(records.length, 8); output[10] = palette.length; output[11] = 4; output.writeFloatLE(1.6, 12);
  let offset = 16;
  for (const color of palette) { Buffer.from(color).copy(output, offset); offset += 4; }
  for (const record of records) {
    output.writeInt8(record.x, offset); output.writeInt8(record.y, offset + 1); output.writeInt8(record.z, offset + 2);
    output[offset + 3] = record.tier; output[offset + 4] = paletteIndex.get(record.color); output[offset + 5] = record.part;
    offset += 8;
  }
  return { output, records: records.length, palette: palette.length, source: entry.geometry.primary.size };
}

const outputDir = path.join(root, 'Cube/lvl27');
for (const entry of context.globalThis.__lvl27) {
  const encoded = encode(entry);
  const slug = entry.world.kind === 'special'
    ? 'void'
    : entry.world.themes.map(id => context.globalThis.__api.themeFor(id).name.toLowerCase()).join('_');
  const filename = `world_${String(entry.world.id).padStart(2, '0')}_${slug}.cubes`;
  fs.writeFileSync(path.join(outputDir, filename), encoded.output);
  console.log(`${filename}: ${encoded.source} c4 cells -> ${encoded.records} records, ${encoded.palette} colours`);
}
