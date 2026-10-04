// Runs the built contract the way the Retium runtime does and checks the
// replies. Needs Node 18 or newer and nothing else.
//
//   cargo build --release --target wasm32-unknown-unknown -p hello_counter
//   node examples/hello_counter/run-as-runtime.mjs
//
// The runtime builds a fresh module for every call, writes the input JSON at
// offset 0 of its memory and calls on_use(0, input_len, input_len + 1024).
// Storage here is a plain map that outlives the calls, as chain state does.

import assert from 'node:assert/strict';
import fs from 'node:fs';

const path = process.argv[2] ?? 'target/wasm32-unknown-unknown/release/hello_counter.wasm';
const module = new WebAssembly.Module(fs.readFileSync(path));

const state = new Map();
let memory;

const text = (ptr, len) => Buffer.from(new Uint8Array(memory.buffer, ptr, len)).toString();
const put = (ptr, max, value) => {
  const bytes = Buffer.from(value);
  const len = Math.min(bytes.length, max);
  new Uint8Array(memory.buffer, ptr, len).set(bytes.subarray(0, len));
  return len;
};

const env = {
  write_state: (kp, kl, vp, vl) => void state.set(text(kp, kl), text(vp, vl)),
  read_state: (kp, kl, out, max) => {
    const value = state.get(text(kp, kl));
    return value === undefined ? 0 : put(out, max, value);
  },
  emit_event: () => 0,
};
// The counter uses storage and events only. Anything else it imports is
// answered with "not available".
for (const item of WebAssembly.Module.imports(module)) {
  if (!(item.name in env)) env[item.name] = () => -1;
}

function call(caller, action, args = []) {
  const instance = new WebAssembly.Instance(module, { env });
  memory = instance.exports.memory;
  const input = Buffer.from(
    JSON.stringify({ action, caller, args, token_context: null, value: '0' }),
  );
  new Uint8Array(memory.buffer, 0, input.length).set(input);
  const out = input.length + 1024;
  const len = instance.exports.on_use(0, input.length, out);
  return JSON.parse(text(out, len));
}

const alice = 'rete' + '1'.repeat(68);
const bob = 'rete' + '2'.repeat(68);

assert.deepEqual(call(alice, 'init'), { ok: true, data: alice });
assert.equal(call(bob, 'init').ok, false, 'a second init is refused');
assert.deepEqual(call(bob, 'bump', ['5']), { ok: true, data: '5' });
assert.deepEqual(call(bob, 'bump'), { ok: true, data: '6' });
assert.deepEqual(call(bob, 'value'), { ok: true, data: '6' });
assert.deepEqual(call(bob, 'owner'), { ok: true, data: alice });
assert.equal(call(bob, 'reset').ok, false, 'only the owner resets');
assert.equal(call(alice, 'reset').ok, true);
assert.deepEqual(call(bob, 'value'), { ok: true, data: '0' });
assert.equal(call(bob, 'nope').ok, false, 'an unknown action is an error');

console.log('hello_counter answers correctly when run as the runtime runs it');
