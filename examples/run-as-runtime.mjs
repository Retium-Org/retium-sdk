// Runs a built counter contract the way the Retium runtime does and checks the
// replies. Needs Node 18 or newer and nothing else.
//
//   node examples/run-as-runtime.mjs target/wasm32-unknown-unknown/release/hello_counter.wasm
//   node examples/run-as-runtime.mjs examples/hello_counter_c/counter.wasm
//
// The runtime builds a fresh module for every call, writes the input JSON at
// offset 0 of its memory and calls on_use(0, input_len, input_len + 1024).
// Storage here is a plain map that outlives the calls, as chain state does.
// Every counter example in this folder answers the same way, so one script
// checks them all.

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
  assert.equal(item.module, 'env', `the chain only provides imports from "env", not "${item.module}"`);
  if (!(item.name in env)) env[item.name] = () => -1;
}
assert.ok(WebAssembly.Module.exports(module).some((e) => e.name === 'memory' && e.kind === 'memory'), 'the module must export its memory');
assert.ok(WebAssembly.Module.exports(module).some((e) => e.name === 'on_use' && e.kind === 'function'), 'the module must export on_use');

function call(caller, action, args = []) {
  const instance = new WebAssembly.Instance(module, { env });
  memory = instance.exports.memory;
  const input = Buffer.from(
    JSON.stringify({ action, args, caller, token_context: null, value: '0' }),
  );
  new Uint8Array(memory.buffer, 0, input.length).set(input);
  const out = input.length + 1024;
  const len = instance.exports.on_use(0, input.length, out);
  assert.ok(len >= 0 && len <= 4096, `reply length ${len} is outside 0..4096`);
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
assert.equal(call(bob, 'bump', ['0']).ok, false, 'a zero step is refused');
assert.equal(call(bob, 'bump', ['1001']).ok, false, 'a step over 1000 is refused');
assert.equal(call(bob, 'bump', ['ten']).ok, false, 'a step that is not a number is refused');
const unknown = call(bob, 'nope');
assert.equal(unknown.ok, false, 'an unknown action is an error');
assert.equal(typeof unknown.error, 'string', 'an error reply names the error');

// A long argument makes the chain write far into memory: the input lands at
// offset 0 and the reply buffer right after it. A contract whose own data sits
// low in memory is overwritten and answers garbage. Keep everything above 64 KiB.
assert.deepEqual(call(bob, 'bump', ['3', 'x'.repeat(3000)]), { ok: true, data: '3' });
assert.deepEqual(call(bob, 'value', ['x'.repeat(3000)]), { ok: true, data: '3' });

console.log(`${path} answers correctly when run as the runtime runs it`);
