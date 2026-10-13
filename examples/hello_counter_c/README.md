# hello_counter in C and C++

The same counter as [`hello_counter`](../hello_counter), written in C with no libc. Actions: `init`, `bump [step]`, `value`, `owner` and `reset`. The source is `counter.c`, one file, and it is valid C++ as well (see below).

## Build

With clang 15 or newer (the LLVM release includes `wasm-ld`):

```bash
clang --target=wasm32 -Oz -nostdlib -Wl,--no-entry -Wl,--export=on_use \
      -Wl,--stack-first -Wl,-z,stack-size=1048576 -o counter.wasm counter.c
```

Or with Zig, which ships a C compiler and needs no layout flags:

```bash
zig cc --target=wasm32-freestanding -Oz -nostdlib -Wl,--no-entry -Wl,--export=on_use -o counter.wasm counter.c
```

For C++, use `clang++` or `zig c++` with the same flags plus `-fno-exceptions -fno-rtti` (and `-x c++` to compile this `.c` file as C++). There is no C++ runtime on the chain, so no exceptions, no `new` and no standard library. The chain's functions and `on_use` are declared `extern "C"` so the linker finds them under their plain names.

```bash
zig c++ --target=wasm32-freestanding -Oz -nostdlib -fno-exceptions -fno-rtti -Wl,--no-entry -Wl,--export=on_use -x c++ counter.c -o counter.wasm
```

Then run it the way the chain does:

```bash
node ../run-as-runtime.mjs counter.wasm
```

## Deploy

```bash
retium deploy counter.wasm --dry-run
retium deploy counter.wasm
```

## What is different from Rust

* The chain writes a call's input at offset 0 of memory and the reply buffer right after it. `--stack-first` with a 1 MiB stack puts this module's data at 1 MiB, the layout Rust and Zig use. Without it `wasm-ld` puts the data at 1024 and a call with a long argument overwrites it.
* Import only the `env` functions listed in [docs/08-wire-reference.md](../../docs/08-wire-reference.md#host-functions). The chain provides nothing else, no libc and no WASI. The file defines its own `memcpy` and `memset` because the compiler may emit calls to them.
* The RCP-1 metadata is a C string marked `used`, so it stays in the binary and its terminating zero ends the JSON.
* This module scores 5, the same as a Rust contract. If you cut it down and the dry run reports a lower score, put that number in the metadata string and pass the same number in a `--metadata` file.
