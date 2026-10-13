;; The Retium hello_counter contract, written by hand in WebAssembly text.
;; No compiler and no language runtime, only the instructions the chain runs.
;;
;; Same actions as examples/hello_counter: init, bump [step], value, owner and
;; reset. Assemble it with wasm-tools (cargo install wasm-tools):
;;
;;   wasm-tools parse counter.wat -o counter.wasm
;;
;; Two things matter for any language that is not Rust.
;;
;; 1. The chain writes the call's input at offset 0 of memory and the reply
;;    buffer right after it. Everything of this module lives at 65536 and above.
;; 2. The chain scores the code and the declared weight must match the score
;;    (`retium deploy --dry-run` prints it). This module is small and scores 4,
;;    so the metadata says 4. Code that scores below 4 must also carry the weight
;;    in the deploy request's metadata, see docs/08-wire-reference.md.
(module
  ;; The functions the chain gives a contract. Import nothing else: the chain
  ;; refuses to instantiate a module that imports anything it does not provide.
  (import "env" "read_state"  (func $read_state  (param i32 i32 i32 i32) (result i32)))
  (import "env" "write_state" (func $write_state (param i32 i32 i32 i32)))
  (import "env" "emit_event"  (func $emit_event  (param i32 i32 i32 i32) (result i32)))

  ;; Two pages: the chain's region below 64 KiB, this module's data above it.
  (memory (export "memory") 2)

  ;; ---- constants, all at 65536 and above ------------------------------------
  (data (i32.const 65536)   "count")
  (data (i32.const 65544)   "owner")
  (data (i32.const 65552)  "{\"ok\":true,\"data\":\"")
  (data (i32.const 65584)  "{\"ok\":false,\"error\":\"")
  (data (i32.const 65616)  "\"action\":\"")
  (data (i32.const 65632)  "\"caller\":\"")
  (data (i32.const 65648) "\"args\":[\"")
  (data (i32.const 65664) "counter_created")
  (data (i32.const 65680) "counter_bumped")
  (data (i32.const 65696) "counter_reset")
  (data (i32.const 65712) "{}")
  (data (i32.const 65728) "init")
  (data (i32.const 65736) "bump")
  (data (i32.const 65744) "value")
  (data (i32.const 65752) "owner")
  (data (i32.const 65760) "reset")
  (data (i32.const 65792) "already initialised")
  (data (i32.const 65824) "step must be between 1 and 1000")
  (data (i32.const 65856) "step must be a number")
  (data (i32.const 65888) "not initialised yet")
  (data (i32.const 65920) "only the owner may reset this counter")
  (data (i32.const 65968) "unknown action, try init, bump, value, owner or reset")
  (data (i32.const 66032) "malformed input")
  ;; RCP-1 metadata. The chain finds the marker in the wasm bytes and reads the
  ;; JSON up to the zero byte.
  (data (i32.const 66048) "RCP_METADATA:{\"rcp_type\":\"counter\",\"traits\":[\"demo\"],\"capabilities\":[\"execute\"],\"lockable_fields\":[],\"evolves_from\":\"\",\"weight\":4,\"version\":\"v1.0.0\"}\00")
  ;; Scratch: 66304 owner (128 bytes), 66432 number text (32), 66496 digits (32).

  ;; The current call's input, and the last field found.
  (global $in (mut i32) (i32.const 0))
  (global $len (mut i32) (i32.const 0))
  (global $fp (mut i32) (i32.const 0))
  (global $fl (mut i32) (i32.const 0))

  ;; ---- helpers ------------------------------------------------------------------

  ;; Offset of needle in hay, or -1.
  (func $find (param $h i32) (param $hl i32) (param $n i32) (param $nl i32) (result i32)
    (local $i i32) (local $j i32)
    (block $done
      (loop $outer
        (br_if $done (i32.gt_s (i32.add (local.get $i) (local.get $nl)) (local.get $hl)))
        (local.set $j (i32.const 0))
        (block $mismatch
          (loop $inner
            (br_if $mismatch (i32.ne
              (i32.load8_u (i32.add (local.get $h) (i32.add (local.get $i) (local.get $j))))
              (i32.load8_u (i32.add (local.get $n) (local.get $j)))))
            (local.set $j (i32.add (local.get $j) (i32.const 1)))
            (br_if $inner (i32.lt_s (local.get $j) (local.get $nl))))
          (return (local.get $i)))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $outer)))
    (i32.const -1))

  ;; Are two byte strings equal?
  (func $eq (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i32)
    (local $i i32)
    (if (i32.ne (local.get $al) (local.get $bl)) (then (return (i32.const 0))))
    (block $done (loop $l
      (br_if $done (i32.ge_s (local.get $i) (local.get $al)))
      (if (i32.ne (i32.load8_u (i32.add (local.get $a) (local.get $i)))
                  (i32.load8_u (i32.add (local.get $b) (local.get $i))))
        (then (return (i32.const 0))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l)))
    (i32.const 1))

  (func $memcpy (param $dst i32) (param $src i32) (param $n i32)
    (local $i i32)
    (block $end (loop $l
      (br_if $end (i32.ge_s (local.get $i) (local.get $n)))
      (i32.store8 (i32.add (local.get $dst) (local.get $i))
                  (i32.load8_u (i32.add (local.get $src) (local.get $i))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l))))

  ;; Offset of the quote that closes the JSON string starting at $p, or -1.
  (func $string_end (param $p i32) (result i32)
    (local $c i32)
    (block $done (loop $l
      (br_if $done (i32.ge_s (local.get $p) (global.get $len)))
      (local.set $c (i32.load8_u (i32.add (global.get $in) (local.get $p))))
      (if (i32.eq (local.get $c) (i32.const 34)) (then (return (local.get $p))))
      (local.set $p (i32.add (local.get $p)
        (select (i32.const 2) (i32.const 1) (i32.eq (local.get $c) (i32.const 92)))))
      (br $l)))
    (i32.const -1))

  ;; Find `"name":"` (or `"args":["`) in the input and remember where its string
  ;; value starts and how long it is. Returns 1 when found.
  (func $field (param $pat i32) (param $pl i32) (result i32)
    (local $at i32) (local $start i32) (local $end i32)
    (local.set $at (call $find (global.get $in) (global.get $len) (local.get $pat) (local.get $pl)))
    (if (i32.lt_s (local.get $at) (i32.const 0)) (then (return (i32.const 0))))
    (local.set $start (i32.add (local.get $at) (local.get $pl)))
    (local.set $end (call $string_end (local.get $start)))
    (if (i32.lt_s (local.get $end) (i32.const 0)) (then (return (i32.const 0))))
    (global.set $fp (i32.add (global.get $in) (local.get $start)))
    (global.set $fl (i32.sub (local.get $end) (local.get $start)))
    (i32.const 1))

  ;; Decimal digits only, 1 to 19 of them. Anything else gives -1.
  (func $parse (param $p i32) (param $n i32) (result i64)
    (local $i i32) (local $v i64) (local $c i32)
    (if (i32.or (i32.le_s (local.get $n) (i32.const 0)) (i32.gt_s (local.get $n) (i32.const 19)))
      (then (return (i64.const -1))))
    (block $end (loop $l
      (br_if $end (i32.ge_s (local.get $i) (local.get $n)))
      (local.set $c (i32.load8_u (i32.add (local.get $p) (local.get $i))))
      (if (i32.or (i32.lt_u (local.get $c) (i32.const 48)) (i32.gt_u (local.get $c) (i32.const 57)))
        (then (return (i64.const -1))))
      (local.set $v (i64.add (i64.mul (local.get $v) (i64.const 10))
                             (i64.extend_i32_u (i32.sub (local.get $c) (i32.const 48)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l)))
    (local.get $v))

  ;; Write $v in decimal at $dst. Returns the number of digits.
  (func $fmt (param $v i64) (param $dst i32) (result i32)
    (local $n i32) (local $i i32)
    (loop $l
      (i32.store8 (i32.add (i32.const 66496) (local.get $n))
        (i32.add (i32.const 48) (i32.wrap_i64 (i64.rem_u (local.get $v) (i64.const 10)))))
      (local.set $n (i32.add (local.get $n) (i32.const 1)))
      (local.set $v (i64.div_u (local.get $v) (i64.const 10)))
      (br_if $l (i64.ne (local.get $v) (i64.const 0))))
    (block $end (loop $r
      (br_if $end (i32.ge_s (local.get $i) (local.get $n)))
      (i32.store8 (i32.add (local.get $dst) (local.get $i))
        (i32.load8_u (i32.add (i32.const 66496)
          (i32.sub (i32.sub (local.get $n) (i32.const 1)) (local.get $i)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $r)))
    (local.get $n))

  ;; ---- storage and events ------------------------------------------------------

  ;; Read a key into $buf. Returns the length, 0 when the key is not set.
  (func $get (param $k i32) (param $kl i32) (param $buf i32) (param $max i32) (result i32)
    (local $n i32)
    (local.set $n (call $read_state (local.get $k) (local.get $kl) (local.get $buf) (local.get $max)))
    (select (i32.const 0) (local.get $n) (i32.lt_s (local.get $n) (i32.const 0))))

  (func $count (result i64)
    (local $n i32) (local $v i64)
    (local.set $n (call $get (i32.const 65536) (i32.const 5) (i32.const 66432) (i32.const 32)))
    (if (i32.eqz (local.get $n)) (then (return (i64.const 0))))
    (local.set $v (call $parse (i32.const 66432) (local.get $n)))
    (select (i64.const 0) (local.get $v) (i64.lt_s (local.get $v) (i64.const 0))))

  (func $store_count (param $v i64)
    (call $write_state (i32.const 65536) (i32.const 5) (i32.const 66432)
      (call $fmt (local.get $v) (i32.const 66432))))

  (func $emit (param $t i32) (param $tl i32)
    (drop (call $emit_event (local.get $t) (local.get $tl) (i32.const 65712) (i32.const 2))))

  ;; ---- replies ----------------------------------------------------------------------

  ;; prefix, then the text, then `"}`. Returns the reply length.
  (func $reply (param $out i32) (param $pre i32) (param $pl i32) (param $s i32) (param $sl i32) (result i32)
    (call $memcpy (local.get $out) (local.get $pre) (local.get $pl))
    (call $memcpy (i32.add (local.get $out) (local.get $pl)) (local.get $s) (local.get $sl))
    (i32.store8 (i32.add (local.get $out) (i32.add (local.get $pl) (local.get $sl))) (i32.const 34))
    (i32.store8 (i32.add (local.get $out) (i32.add (local.get $pl) (i32.add (local.get $sl) (i32.const 1)))) (i32.const 125))
    (i32.add (local.get $pl) (i32.add (local.get $sl) (i32.const 2))))

  (func $ok (param $out i32) (param $s i32) (param $sl i32) (result i32)
    (call $reply (local.get $out) (i32.const 65552) (i32.const 19) (local.get $s) (local.get $sl)))

  (func $ok_num (param $out i32) (param $v i64) (result i32)
    (call $ok (local.get $out) (i32.const 66432) (call $fmt (local.get $v) (i32.const 66432))))

  (func $err (param $out i32) (param $s i32) (param $sl i32) (result i32)
    (call $reply (local.get $out) (i32.const 65584) (i32.const 21) (local.get $s) (local.get $sl)))

  ;; ---- the entry point ---------------------------------------------------------------

  (func (export "on_use") (param $inp i32) (param $inl i32) (param $out i32) (result i32)
    (local $ap i32) (local $al i32) (local $cp i32) (local $cl i32) (local $n i32) (local $step i64) (local $v i64)
    (global.set $in (local.get $inp))
    (global.set $len (local.get $inl))

    (if (i32.eqz (call $field (i32.const 65616) (i32.const 10)))
      (then (return (call $err (local.get $out) (i32.const 66032) (i32.const 15)))))
    (local.set $ap (global.get $fp))
    (local.set $al (global.get $fl))
    (if (i32.eqz (call $field (i32.const 65632) (i32.const 10)))
      (then (return (call $err (local.get $out) (i32.const 66032) (i32.const 15)))))
    (local.set $cp (global.get $fp))
    (local.set $cl (global.get $fl))

    ;; init: record the first caller as the owner. Until this has run, anyone may call it.
    (if (call $eq (local.get $ap) (local.get $al) (i32.const 65728) (i32.const 4))
      (then
        (if (i32.gt_s (call $get (i32.const 65544) (i32.const 5) (i32.const 66304) (i32.const 128)) (i32.const 0))
          (then (return (call $err (local.get $out) (i32.const 65792) (i32.const 19)))))
        (call $write_state (i32.const 65544) (i32.const 5) (local.get $cp) (local.get $cl))
        (call $store_count (i64.const 0))
        (call $emit (i32.const 65664) (i32.const 15))
        (return (call $ok (local.get $out) (local.get $cp) (local.get $cl)))))

    ;; bump [step]: the step is checked before anything is written.
    (if (call $eq (local.get $ap) (local.get $al) (i32.const 65736) (i32.const 4))
      (then
        (local.set $step (i64.const 1))
        (if (call $field (i32.const 65648) (i32.const 9))
          (then
            (local.set $step (call $parse (global.get $fp) (global.get $fl)))
            (if (i64.lt_s (local.get $step) (i64.const 0))
              (then (return (call $err (local.get $out) (i32.const 65856) (i32.const 21)))))))
        (if (i32.or (i64.eqz (local.get $step)) (i64.gt_u (local.get $step) (i64.const 1000)))
          (then (return (call $err (local.get $out) (i32.const 65824) (i32.const 31)))))
        (local.set $v (i64.add (call $count) (local.get $step)))
        (call $store_count (local.get $v))
        (call $emit (i32.const 65680) (i32.const 14))
        (return (call $ok_num (local.get $out) (local.get $v)))))

    ;; value
    (if (call $eq (local.get $ap) (local.get $al) (i32.const 65744) (i32.const 5))
      (then (return (call $ok_num (local.get $out) (call $count)))))

    ;; owner
    (if (call $eq (local.get $ap) (local.get $al) (i32.const 65752) (i32.const 5))
      (then
        (local.set $n (call $get (i32.const 65544) (i32.const 5) (i32.const 66304) (i32.const 128)))
        (return (call $ok (local.get $out) (i32.const 66304) (local.get $n)))))

    ;; reset: only the owner. The caller is proven by the chain.
    (if (call $eq (local.get $ap) (local.get $al) (i32.const 65760) (i32.const 5))
      (then
        (local.set $n (call $get (i32.const 65544) (i32.const 5) (i32.const 66304) (i32.const 128)))
        (if (i32.eqz (local.get $n))
          (then (return (call $err (local.get $out) (i32.const 65888) (i32.const 19)))))
        (if (i32.eqz (call $eq (i32.const 66304) (local.get $n) (local.get $cp) (local.get $cl)))
          (then (return (call $err (local.get $out) (i32.const 65920) (i32.const 37)))))
        (call $store_count (i64.const 0))
        (call $emit (i32.const 65696) (i32.const 13))
        (return (call $ok_num (local.get $out) (i64.const 0)))))

    (call $err (local.get $out) (i32.const 65968) (i32.const 53))))
