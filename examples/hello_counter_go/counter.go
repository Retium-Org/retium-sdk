// The Retium hello_counter contract, in Go, built with TinyGo.
//
// Same actions as examples/hello_counter: init, bump [step], value, owner and
// reset. Build it with TinyGo 0.43 or newer (it needs Go and Binaryen's wasm-opt):
//
//	tinygo build -o counter.wasm -target=wasm-unknown -opt=z -no-debug -panic=trap .
//	node add-start.mjs counter.wasm
//
// The standard Go compiler cannot build this. Its wasm output needs a JavaScript
// or WASI host, and the chain is neither. TinyGo's wasm-unknown target produces
// a module with no imports of its own, which is what the chain runs.
//
// Four things matter here. The first two apply to any language that is not Rust.
//
//  1. The chain writes the call's input at offset 0 of memory and the reply
//     buffer right after it. TinyGo puts a 64 KiB stack first and its data above
//     it, so the chain's bytes sit at the unused bottom of the stack. Keep stack
//     use modest (no deep recursion) and they stay out of each other's way.
//  2. The chain scores the code and the declared weight must match the score
//     (`retium deploy --dry-run` prints it). This module scores 5, like a Rust
//     contract. A smaller module may score less, and must say so.
//  3. TinyGo builds a "reactor" that expects the host to call `_initialize`
//     before any export, and traps otherwise. The chain never calls it, so
//     add-start.mjs makes `_initialize` the module's start function, which
//     runs when the chain instantiates the module.
//  4. Address 0 is Go's nil. No Go pointer may read it, so on_use reads the
//     input from byte 1 (the input is a JSON object, byte 0 is always `{`).
package main

import (
	"strconv"
	"strings"
	"unsafe"
)

// The functions the chain gives a contract. Import nothing else: the chain
// refuses to instantiate a module that imports anything it does not provide.
//
//go:wasmimport env read_state
func read_state(keyPtr unsafe.Pointer, keyLen int32, outPtr unsafe.Pointer, outMax int32) int32

//go:wasmimport env write_state
func write_state(keyPtr unsafe.Pointer, keyLen int32, valPtr unsafe.Pointer, valLen int32)

//go:wasmimport env emit_event
func emit_event(typePtr unsafe.Pointer, typeLen int32, dataPtr unsafe.Pointer, dataLen int32) int32

// RCP-1 metadata. The chain finds the marker in the wasm bytes and reads the
// JSON up to the zero byte. The exported function below is what keeps the
// bytes in the binary; the chain never calls it.
const rcpMetadata = "RCP_METADATA:{\"rcp_type\":\"counter\",\"traits\":[\"demo\"]," +
	"\"capabilities\":[\"execute\"],\"lockable_fields\":[],\"evolves_from\":\"\"," +
	"\"weight\":5,\"version\":\"v1.0.0\"}\x00"

//go:wasmexport rcp_metadata_ptr
func rcpMetadataPtr() int32 {
	return int32(uintptr(unsafe.Pointer(unsafe.StringData(rcpMetadata))))
}

const (
	ownerKey = "owner"
	countKey = "count"
	maxStep  = 1000
	maxReply = 4096
)

// main is required by the compiler and never runs on the chain.
func main() {}

//go:wasmexport on_use
func onUse(inputPtr, inputLen, outputPtr int32) int32 {
	// The chain puts the input at offset 0, and address 0 is Go's nil, which no
	// pointer may read through. The input is a JSON object, so byte 0 is `{`
	// and nothing is lost by reading from byte 1.
	input := ""
	if inputLen > 1 {
		input = unsafe.String((*byte)(unsafe.Pointer(uintptr(inputPtr)+1)), int(inputLen-1))
	}
	action, ok1 := field(input, "action")
	caller, ok2 := field(input, "caller")

	var reply string
	switch {
	case !ok1 || !ok2:
		reply = errorReply("malformed input")
	case action == "init":
		reply = initCounter(caller)
	case action == "bump":
		reply = bump(caller, firstArg(input))
	case action == "value":
		reply = okReply(strconv.FormatUint(countNow(), 10))
	case action == "owner":
		reply = okReply(get(ownerKey))
	case action == "reset":
		reply = reset(caller)
	default:
		reply = errorReply("unknown action " + jsonString(action) + ", try init, bump, value, owner or reset")
	}

	if len(reply) > maxReply {
		return -1
	}
	out := unsafe.Slice((*byte)(unsafe.Pointer(uintptr(outputPtr))), maxReply)
	return int32(copy(out, reply))
}

// Record the first caller as the owner. Until this has run, anyone may call it.
func initCounter(caller string) string {
	if get(ownerKey) != "" {
		return errorReply("already initialised")
	}
	set(ownerKey, caller)
	set(countKey, "0")
	emit("counter_created", "owner", caller)
	return okReply(caller)
}

// Add to the counter. The step is checked before anything is written.
func bump(caller string, arg *string) string {
	step := uint64(1)
	if arg != nil {
		n, err := strconv.ParseUint(*arg, 10, 64)
		if err != nil || !isDigits(*arg) {
			return errorReply("step must be a number")
		}
		step = n
	}
	if step == 0 || step > maxStep {
		return errorReply("step must be between 1 and 1000")
	}
	before := countNow()
	total := before + step
	if total < before {
		return errorReply("counter overflow")
	}
	text := strconv.FormatUint(total, 10)
	set(countKey, text)
	emit("counter_bumped", "by", caller, "step", strconv.FormatUint(step, 10), "total", text)
	return okReply(text)
}

// Only the owner may reset. The caller is proven by the chain.
func reset(caller string) string {
	owner := get(ownerKey)
	if owner == "" {
		return errorReply("not initialised yet")
	}
	if caller != owner {
		return errorReply("only the owner may reset this counter")
	}
	set(countKey, "0")
	emit("counter_reset", "by", caller)
	return okReply("0")
}

func countNow() uint64 {
	raw := get(countKey)
	if !isDigits(raw) {
		return 0
	}
	n, err := strconv.ParseUint(raw, 10, 64)
	if err != nil {
		return 0
	}
	return n
}

// ---- storage and events ------------------------------------------------------

func get(key string) string {
	var buf [256]byte
	n := read_state(unsafe.Pointer(unsafe.StringData(key)), int32(len(key)), unsafe.Pointer(&buf[0]), int32(len(buf)))
	if n <= 0 {
		return ""
	}
	return string(buf[:n])
}

func set(key, value string) {
	write_state(unsafe.Pointer(unsafe.StringData(key)), int32(len(key)),
		unsafe.Pointer(unsafe.StringData(value)), int32(len(value)))
}

// Emit an event whose payload is a JSON object built from name/value pairs.
func emit(kind string, pairs ...string) {
	var b strings.Builder
	b.WriteByte('{')
	for i := 0; i+1 < len(pairs); i += 2 {
		if i > 0 {
			b.WriteByte(',')
		}
		b.WriteString(jsonString(pairs[i]))
		b.WriteByte(':')
		b.WriteString(jsonString(pairs[i+1]))
	}
	b.WriteByte('}')
	data := b.String()
	emit_event(unsafe.Pointer(unsafe.StringData(kind)), int32(len(kind)),
		unsafe.Pointer(unsafe.StringData(data)), int32(len(data)))
}

// ---- replies -------------------------------------------------------------------

func okReply(data string) string {
	return "{\"ok\":true,\"data\":" + jsonString(data) + "}"
}

func errorReply(message string) string {
	return "{\"ok\":false,\"error\":" + jsonString(message) + "}"
}

// ---- a little JSON ----------------------------------------------------------------

// Quote and escape a string so it is safe inside JSON.
func jsonString(value string) string {
	var b strings.Builder
	b.WriteByte('"')
	for i := 0; i < len(value); i++ {
		c := value[i]
		switch {
		case c == '"':
			b.WriteString("\\\"")
		case c == '\\':
			b.WriteString("\\\\")
		case c == '\n':
			b.WriteString("\\n")
		case c == '\r':
			b.WriteString("\\r")
		case c == '\t':
			b.WriteString("\\t")
		case c < 0x20:
			b.WriteString("\\u00")
			b.WriteByte("0123456789abcdef"[c>>4])
			b.WriteByte("0123456789abcdef"[c&15])
		default:
			b.WriteByte(c)
		}
	}
	b.WriteByte('"')
	return b.String()
}

// The string value of `"name":"..."` in the input.
func field(input, name string) (string, bool) {
	pattern := "\"" + name + "\":\""
	at := strings.Index(input, pattern)
	if at < 0 {
		return "", false
	}
	start := at + len(pattern)
	end := stringEnd(input, start)
	if end < 0 {
		return "", false
	}
	return unescape(input[start:end]), true
}

// The first element of `"args":[...]`, or nil when there is none.
func firstArg(input string) *string {
	at := strings.Index(input, "\"args\":[")
	if at < 0 {
		return nil
	}
	start := at + 8
	if start >= len(input) || input[start] != '"' {
		return nil
	}
	end := stringEnd(input, start+1)
	if end < 0 {
		return nil
	}
	s := unescape(input[start+1 : end])
	return &s
}

func stringEnd(s string, from int) int {
	for i := from; i < len(s); i++ {
		switch s[i] {
		case '\\':
			i++
		case '"':
			return i
		}
	}
	return -1
}

func unescape(s string) string {
	if strings.IndexByte(s, '\\') < 0 {
		return s
	}
	var b strings.Builder
	for i := 0; i < len(s); i++ {
		c := s[i]
		if c == '\\' && i+1 < len(s) {
			i++
			switch s[i] {
			case 'n':
				b.WriteByte('\n')
			case 't':
				b.WriteByte('\t')
			case 'r':
				b.WriteByte('\r')
			default:
				b.WriteByte(s[i])
			}
			continue
		}
		b.WriteByte(c)
	}
	return b.String()
}

func isDigits(s string) bool {
	if len(s) == 0 || len(s) > 20 {
		return false
	}
	for i := 0; i < len(s); i++ {
		if s[i] < '0' || s[i] > '9' {
			return false
		}
	}
	return true
}
