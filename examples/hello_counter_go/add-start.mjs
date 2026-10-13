// Makes the module run TinyGo's `_initialize` when it is instantiated.
//
// TinyGo's wasm-unknown target builds a "reactor": the host is expected to
// call the exported `_initialize` once before any other export, and an export
// called before that traps. The chain never calls it. A WebAssembly start
// section runs a function at instantiation, which is exactly the moment we
// need, so this script points one at `_initialize`.
//
//   node add-start.mjs counter.wasm
import fs from 'node:fs';

const path = process.argv[2];
const wasm = fs.readFileSync(path);

let p = 8;
const leb = () => { let r = 0, s = 0, b; do { b = wasm[p++]; r |= (b & 0x7f) << s; s += 7; } while (b & 0x80); return r >>> 0; };
const encode = (n) => { const out = []; do { let b = n & 0x7f; n >>>= 7; if (n) b |= 0x80; out.push(b); } while (n); return Buffer.from(out); };

let exportEnd = -1, initIndex = -1;
while (p < wasm.length) {
  const id = wasm[p++]; const size = leb(); const end = p + size;
  if (id === 8) { console.log(`${path} already has a start section`); process.exit(0); }
  if (id === 7) {
    const count = leb();
    for (let i = 0; i < count; i++) {
      const len = leb(); const name = wasm.subarray(p, p + len).toString(); p += len;
      const kind = wasm[p++]; const index = leb();
      if (name === '_initialize' && kind === 0) initIndex = index;
    }
    exportEnd = end;
  }
  p = end;
}
if (initIndex < 0) { console.error(`${path} has no _initialize export, nothing to do`); process.exit(1); }

const body = encode(initIndex);
const section = Buffer.concat([Buffer.from([8]), encode(body.length), body]);
fs.writeFileSync(path, Buffer.concat([wasm.subarray(0, exportEnd), section, wasm.subarray(exportEnd)]));
console.log(`${path}: _initialize (function ${initIndex}) now runs at instantiation`);
