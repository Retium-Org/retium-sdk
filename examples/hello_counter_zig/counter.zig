//! The Retium hello_counter contract, in Zig.
//!
//! Same actions as examples/hello_counter: init, bump [step], value, owner and
//! reset. Build it with Zig 0.17 or newer:
//!
//!   zig build-exe counter.zig -target wasm32-freestanding -OReleaseSmall \
//!       -fno-entry --export=on_use -femit-bin=counter.wasm
//!
//! Two things matter for any language that is not Rust.
//!
//! 1. The chain writes the call's input at offset 0 of memory and the reply
//!    buffer right after it. Zig puts the stack first and its data at 1 MiB, so
//!    nothing of this module lives in the chain's way. Rust does the same.
//! 2. The chain scores the code and the declared weight must match the score
//!    (`retium deploy --dry-run` prints it). This module scores 5, like a Rust
//!    contract. A smaller module may score less, and must say so.

const std = @import("std");

// The functions the chain gives a contract. Import nothing else: the chain
// refuses to instantiate a module that imports anything it does not provide.
extern "env" fn read_state(key: [*]const u8, key_len: i32, out: [*]u8, out_max: i32) i32;
extern "env" fn write_state(key: [*]const u8, key_len: i32, val: [*]const u8, val_len: i32) void;
extern "env" fn emit_event(kind: [*]const u8, kind_len: i32, data: [*]const u8, data_len: i32) i32;

// RCP-1 metadata. The chain finds the marker in the wasm bytes and reads the
// JSON up to the zero byte. `on_use` touches it so the compiler keeps it.
const RCP_METADATA: [:0]const u8 =
    "RCP_METADATA:{\"rcp_type\":\"counter\",\"traits\":[\"demo\"]," ++
    "\"capabilities\":[\"execute\"],\"lockable_fields\":[],\"evolves_from\":\"\"," ++
    "\"weight\":5,\"version\":\"v1.0.0\"}";

const OWNER = "owner";
const COUNT = "count";
const MAX_STEP: u64 = 1000;
const MAX_REPLY = 4096;

export fn on_use(input_ptr: [*]const u8, input_len: i32, output_ptr: [*]u8) i32 {
    std.mem.doNotOptimizeAway(RCP_METADATA);
    const input = input_ptr[0..@intCast(input_len)];
    const out = output_ptr[0..MAX_REPLY];

    const action = field(input, "action") orelse return err(out, "malformed input");
    const caller = field(input, "caller") orelse return err(out, "malformed input");

    if (std.mem.eql(u8, action, "init")) return init(out, caller);
    if (std.mem.eql(u8, action, "bump")) return bump(out, caller, firstArg(input));
    if (std.mem.eql(u8, action, "value")) return okNumber(out, countNow());
    if (std.mem.eql(u8, action, "owner")) {
        var buf: [256]u8 = undefined;
        return ok(out, get(OWNER, &buf));
    }
    if (std.mem.eql(u8, action, "reset")) return reset(out, caller);
    return err(out, "unknown action, try init, bump, value, owner or reset");
}

/// Record the first caller as the owner. Until this has run, anyone may call it.
fn init(out: []u8, caller: []const u8) i32 {
    var buf: [256]u8 = undefined;
    if (get(OWNER, &buf).len != 0) return err(out, "already initialised");
    set(OWNER, caller);
    set(COUNT, "0");
    emit("counter_created", &.{ .{ "owner", caller } });
    return ok(out, caller);
}

/// Add to the counter. The step is checked before anything is written.
fn bump(out: []u8, caller: []const u8, arg: ?[]const u8) i32 {
    var step: u64 = 1;
    if (arg) |text| step = parseNumber(text) orelse return err(out, "step must be a number");
    if (step == 0 or step > MAX_STEP) return err(out, "step must be between 1 and 1000");
    const before = countNow();
    const total = std.math.add(u64, before, step) catch return err(out, "counter overflow");

    var num: [24]u8 = undefined;
    const total_text = number(&num, total);
    set(COUNT, total_text);
    var step_buf: [24]u8 = undefined;
    emit("counter_bumped", &.{ .{ "by", caller }, .{ "step", number(&step_buf, step) }, .{ "total", total_text } });
    return ok(out, total_text);
}

/// Only the owner may reset. The caller is proven by the chain.
fn reset(out: []u8, caller: []const u8) i32 {
    var buf: [256]u8 = undefined;
    const owner = get(OWNER, &buf);
    if (owner.len == 0) return err(out, "not initialised yet");
    if (!std.mem.eql(u8, owner, caller)) return err(out, "only the owner may reset this counter");
    set(COUNT, "0");
    emit("counter_reset", &.{ .{ "by", caller } });
    return ok(out, "0");
}

fn countNow() u64 {
    var buf: [32]u8 = undefined;
    return parseNumber(get(COUNT, &buf)) orelse 0;
}

// ---- storage and events ------------------------------------------------------

fn get(key: []const u8, buf: []u8) []const u8 {
    const n = read_state(key.ptr, @intCast(key.len), buf.ptr, @intCast(buf.len));
    if (n <= 0) return "";
    return buf[0..@intCast(n)];
}

fn set(key: []const u8, value: []const u8) void {
    write_state(key.ptr, @intCast(key.len), value.ptr, @intCast(value.len));
}

const Pair = struct { []const u8, []const u8 };

/// Emit an event whose payload is a JSON object built from name/value pairs.
fn emit(kind: []const u8, pairs: []const Pair) void {
    var data: [1024]u8 = undefined;
    var w: usize = 0;
    data[w] = '{';
    w += 1;
    for (pairs, 0..) |pair, i| {
        if (w + pair[0].len + pair[1].len + 8 > data.len) break;
        if (i > 0) {
            data[w] = ',';
            w += 1;
        }
        w += jsonString(data[w..], pair[0]);
        data[w] = ':';
        w += 1;
        w += jsonString(data[w..], pair[1]);
    }
    data[w] = '}';
    w += 1;
    _ = emit_event(kind.ptr, @intCast(kind.len), &data, @intCast(w));
}

// ---- replies -------------------------------------------------------------------

fn ok(out: []u8, data: []const u8) i32 {
    const head = "{\"ok\":true,\"data\":";
    if (head.len + data.len * 2 + 3 > out.len) return -1;
    @memcpy(out[0..head.len], head);
    var w = head.len;
    w += jsonString(out[w..], data);
    out[w] = '}';
    return @intCast(w + 1);
}

fn okNumber(out: []u8, value: u64) i32 {
    var num: [24]u8 = undefined;
    return ok(out, number(&num, value));
}

fn err(out: []u8, message: []const u8) i32 {
    const head = "{\"ok\":false,\"error\":";
    @memcpy(out[0..head.len], head);
    var w = head.len;
    w += jsonString(out[w..], message);
    out[w] = '}';
    return @intCast(w + 1);
}

// ---- a little JSON and numbers --------------------------------------------------

/// Write a JSON string, escaping quotes and backslashes. Returns bytes written.
fn jsonString(out: []u8, s: []const u8) usize {
    var w: usize = 0;
    out[w] = '"';
    w += 1;
    for (s) |c| {
        if (c == '"' or c == '\\') {
            out[w] = '\\';
            w += 1;
        }
        out[w] = c;
        w += 1;
    }
    out[w] = '"';
    return w + 1;
}

/// The end of the JSON string that starts at `start`, or null if it never closes.
fn stringEnd(input: []const u8, start: usize) ?usize {
    var i = start;
    while (i < input.len) {
        if (input[i] == '"') return i;
        i += if (input[i] == '\\') 2 else 1;
    }
    return null;
}

/// The string value of `"name":"..."` in the input.
fn field(input: []const u8, comptime name: []const u8) ?[]const u8 {
    const pattern = "\"" ++ name ++ "\":\"";
    const at = std.mem.indexOf(u8, input, pattern) orelse return null;
    const start = at + pattern.len;
    const end = stringEnd(input, start) orelse return null;
    return input[start..end];
}

/// The first element of `"args":[...]`, or null when there is none.
fn firstArg(input: []const u8) ?[]const u8 {
    const pattern = "\"args\":[";
    const at = std.mem.indexOf(u8, input, pattern) orelse return null;
    const start = at + pattern.len;
    if (start >= input.len or input[start] != '"') return null;
    const end = stringEnd(input, start + 1) orelse return null;
    return input[start + 1 .. end];
}

/// Decimal digits only, at most 20 of them.
fn parseNumber(text: []const u8) ?u64 {
    if (text.len == 0 or text.len > 20) return null;
    var v: u64 = 0;
    for (text) |c| {
        if (c < '0' or c > '9') return null;
        v = std.math.mul(u64, v, 10) catch return null;
        v = std.math.add(u64, v, c - '0') catch return null;
    }
    return v;
}

fn number(buf: *[24]u8, value: u64) []const u8 {
    var v = value;
    var i: usize = buf.len;
    while (true) {
        i -= 1;
        buf[i] = @intCast('0' + v % 10);
        v /= 10;
        if (v == 0) break;
    }
    return buf[i..];
}
