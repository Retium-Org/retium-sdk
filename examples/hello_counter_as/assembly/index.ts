// The Retium hello_counter contract, in AssemblyScript.
//
// Same actions as examples/hello_counter: init, bump [step], value, owner and
// reset. Build with `npm install && npm run build`, check with `npm run check`.
//
// Two things matter for any language that is not Rust.
//
// 1. The chain writes the call's input at offset 0 of memory and the reply
//    buffer right after it. asconfig.json sets memoryBase to 65536 so nothing
//    of this module lives below 64 KiB.
// 2. The chain scores the code and the declared weight must match the score
//    (`retium deploy --dry-run` prints it). With its runtime this module scores
//    5, like a Rust contract. A smaller module may score less, and must say so.

// The functions the chain gives a contract. Import nothing else: the chain
// refuses to instantiate a module that imports anything it does not provide.
// asconfig.json also disables the `abort` import AssemblyScript adds by default.
@external("env", "read_state")
declare function read_state(keyPtr: usize, keyLen: i32, outPtr: usize, outMax: i32): i32;
@external("env", "write_state")
declare function write_state(keyPtr: usize, keyLen: i32, valPtr: usize, valLen: i32): void;
@external("env", "emit_event")
declare function emit_event(typePtr: usize, typeLen: i32, dataPtr: usize, dataLen: i32): i32;

// RCP-1 metadata as UTF-8 bytes in a data segment, where the chain looks for it.
// AssemblyScript strings are UTF-16 in memory, so a string constant will not do.
// The bytes spell the marker, the JSON and a zero byte:
//
//   RCP_METADATA:{"rcp_type":"counter","traits":["demo"],"capabilities":["execute"],
//                 "lockable_fields":[],"evolves_from":"","weight":5,"version":"v1.0.0"}\0
//
// To change it, edit metadata.mjs, run `node metadata.mjs` and paste the output here.
export const RCP_METADATA: usize = memory.data<u8>([
  82, 67, 80, 95, 77, 69, 84, 65, 68, 65, 84, 65, 58, 123, 34, 114, 99, 112, 95, 116, 121, 112, 101, 34, 58, 34, 99, 111, 117, 110, 116, 101, 114, 34, 44, 34, 116, 114, 97, 105, 116, 115, 34, 58, 91, 34, 100, 101, 109, 111, 34, 93, 44, 34, 99, 97, 112, 97, 98, 105, 108, 105, 116, 105, 101, 115, 34, 58, 91, 34, 101, 120, 101, 99, 117, 116, 101, 34, 93, 44, 34, 108, 111, 99, 107, 97, 98, 108, 101, 95, 102, 105, 101, 108, 100, 115, 34, 58, 91, 93, 44, 34, 101, 118, 111, 108, 118, 101, 115, 95, 102, 114, 111, 109, 34, 58, 34, 34, 44, 34, 119, 101, 105, 103, 104, 116, 34, 58, 53, 44, 34, 118, 101, 114, 115, 105, 111, 110, 34, 58, 34, 118, 49, 46, 48, 46, 48, 34, 125, 0
]);

const OWNER = "owner";
const COUNT = "count";
const MAX_STEP: u64 = 1000;
const MAX_REPLY = 4096;

export function on_use(inputPtr: i32, inputLen: i32, outputPtr: i32): i32 {
  const input = String.UTF8.decodeUnsafe(<usize>inputPtr, <usize>inputLen);
  const action = field(input, "action");
  const caller = field(input, "caller");

  let reply: string;
  if (action == "init") reply = init(caller);
  else if (action == "bump") reply = bump(caller, firstArg(input));
  else if (action == "value") reply = ok(countNow().toString());
  else if (action == "owner") reply = ok(get(OWNER));
  else if (action == "reset") reply = reset(caller);
  else reply = error("unknown action " + jsonString(action) + ", try init, bump, value, owner or reset");

  return write(reply, outputPtr);
}

/** Record the first caller as the owner. Until this has run, anyone may call it. */
function init(caller: string): string {
  if (get(OWNER).length > 0) return error("already initialised");
  set(OWNER, caller);
  set(COUNT, "0");
  emit("counter_created", ["owner", caller]);
  return ok(caller);
}

/** Add to the counter. The step is checked before anything is written. */
function bump(caller: string, arg: string | null): string {
  let step: u64 = 1;
  if (arg !== null) {
    if (!isDigits(arg)) return error("step must be a number");
    step = U64.parseInt(arg, 10);
  }
  if (step == 0 || step > MAX_STEP) return error("step must be between 1 and " + MAX_STEP.toString());
  const before = countNow();
  const total = before + step;
  if (total < before) return error("counter overflow");
  set(COUNT, total.toString());
  emit("counter_bumped", ["by", caller, "step", step.toString(), "total", total.toString()]);
  return ok(total.toString());
}

/** Only the owner may reset. The caller is proven by the chain. */
function reset(caller: string): string {
  const owner = get(OWNER);
  if (owner.length == 0) return error("not initialised yet");
  if (caller != owner) return error("only the owner may reset this counter");
  set(COUNT, "0");
  emit("counter_reset", ["by", caller]);
  return ok("0");
}

function countNow(): u64 {
  const raw = get(COUNT);
  return raw.length > 0 && isDigits(raw) ? U64.parseInt(raw, 10) : 0;
}

// ---- storage and events ----------------------------------------------------

function get(key: string): string {
  const k = String.UTF8.encode(key);
  const out = new ArrayBuffer(256);
  const n = read_state(changetype<usize>(k), k.byteLength, changetype<usize>(out), out.byteLength);
  return n <= 0 ? "" : String.UTF8.decodeUnsafe(changetype<usize>(out), <usize>n);
}

function set(key: string, value: string): void {
  const k = String.UTF8.encode(key);
  const v = String.UTF8.encode(value);
  write_state(changetype<usize>(k), k.byteLength, changetype<usize>(v), v.byteLength);
}

/** Emit an event whose payload is a JSON object built from name/value pairs. */
function emit(type: string, pairs: string[]): void {
  let data = "{";
  for (let i = 0; i + 1 < pairs.length; i += 2) {
    if (i > 0) data += ",";
    data += jsonString(pairs[i]) + ":" + jsonString(pairs[i + 1]);
  }
  data += "}";
  const t = String.UTF8.encode(type);
  const d = String.UTF8.encode(data);
  emit_event(changetype<usize>(t), t.byteLength, changetype<usize>(d), d.byteLength);
}

// ---- replies -----------------------------------------------------------------

function ok(data: string): string {
  return '{"ok":true,"data":' + jsonString(data) + "}";
}

function error(message: string): string {
  return '{"ok":false,"error":' + jsonString(message) + "}";
}

function write(reply: string, outputPtr: i32): i32 {
  const b = String.UTF8.encode(reply);
  if (b.byteLength > MAX_REPLY) return -1;
  memory.copy(<usize>outputPtr, changetype<usize>(b), <usize>b.byteLength);
  return b.byteLength;
}

// ---- a little JSON ----------------------------------------------------------

/** Quote and escape a string so it is safe inside JSON. */
function jsonString(value: string): string {
  let out = '"';
  for (let i = 0; i < value.length; i++) {
    const c = value.charCodeAt(i);
    if (c == 34) out += '\\"';
    else if (c == 92) out += "\\\\";
    else if (c == 10) out += "\\n";
    else if (c == 13) out += "\\r";
    else if (c == 9) out += "\\t";
    else if (c < 32) out += "\\u" + ("000" + c.toString(16)).slice(-4);
    else out += String.fromCharCode(c);
  }
  return out + '"';
}

/** The string value of `"name":"..."` in the input, or "" when absent. */
function field(input: string, name: string): string {
  const pattern = '"' + name + '":"';
  const at = input.indexOf(pattern);
  if (at < 0) return "";
  const start = at + pattern.length;
  const end = closingQuote(input, start);
  return end < 0 ? "" : unescape(input.substring(start, end));
}

/** The first element of `"args":[...]`, or null when there is none. */
function firstArg(input: string): string | null {
  const at = input.indexOf('"args":[');
  if (at < 0) return null;
  const start = at + 8;
  if (start >= input.length || input.charCodeAt(start) != 34) return null;
  const end = closingQuote(input, start + 1);
  return end < 0 ? null : unescape(input.substring(start + 1, end));
}

function closingQuote(s: string, from: i32): i32 {
  for (let i = from; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c == 92) i++;
    else if (c == 34) return i;
  }
  return -1;
}

function unescape(s: string): string {
  if (s.indexOf("\\") < 0) return s;
  let out = "";
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c == 92 && i + 1 < s.length) {
      const n = s.charCodeAt(++i);
      if (n == 110) out += "\n";
      else if (n == 116) out += "\t";
      else if (n == 114) out += "\r";
      else out += String.fromCharCode(n);
    } else out += String.fromCharCode(c);
  }
  return out;
}

function isDigits(s: string): bool {
  if (s.length == 0 || s.length > 20) return false;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c < 48 || c > 57) return false;
  }
  return true;
}
