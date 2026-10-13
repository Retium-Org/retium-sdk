/* The Retium hello_counter contract, in C, with no libc.
 *
 * Same actions as examples/hello_counter: init, bump [step], value, owner and
 * reset. Build it with clang 15 or newer, or with `zig cc` (see README.md):
 *
 *   clang --target=wasm32 -Oz -nostdlib -Wl,--no-entry -Wl,--export=on_use \
 *         -Wl,--stack-first -Wl,-z,stack-size=1048576 -o counter.wasm counter.c
 *
 * It is also valid C++. Build it the same way with clang++ (or `zig c++`) and
 * `-fno-exceptions -fno-rtti -x c++`: there is no C++ runtime on the chain, so
 * no exceptions, no `new` and no standard library. The chain's functions and
 * `on_use` are declared `extern "C"` so their symbols are not mangled.
 *
 * Two things matter for any language that is not Rust.
 *
 * 1. The chain writes the call's input at offset 0 of memory and the reply
 *    buffer right after it. `--stack-first` with a 1 MiB stack puts this
 *    module's data at 1 MiB, as Rust and Zig do, out of the chain's way.
 *    Without it wasm-ld puts the data at 1024 and a long argument overwrites
 *    it. `zig cc` lays memory out this way by default and needs no flag.
 * 2. The chain scores the code and the declared weight must match the score
 *    (`retium deploy --dry-run` prints it). This module scores 5, like a Rust
 *    contract. A smaller module may score less, and must say so.
 */

/* The functions the chain gives a contract. Import nothing else: the chain
 * refuses to instantiate a module that imports anything it does not provide. */
#ifdef __cplusplus
extern "C" {
#endif
#define RETIUM(name) __attribute__((import_module("env"), import_name(name)))
RETIUM("read_state")  int  read_state(const char *key, int key_len, char *out, int out_max);
RETIUM("write_state") void write_state(const char *key, int key_len, const char *val, int val_len);
RETIUM("emit_event")  int  emit_event(const char *type, int type_len, const char *data, int data_len);
__attribute__((export_name("on_use")))
int on_use(const char *in, int len, char *out);
#ifdef __cplusplus
}
#endif

/* RCP-1 metadata. The chain finds the marker in the wasm bytes and the zero
 * byte that ends a C string ends the JSON. `used` keeps it in the binary. */
__attribute__((used)) static const char RCP_METADATA[] =
    "RCP_METADATA:{\"rcp_type\":\"counter\",\"traits\":[\"demo\"],"
    "\"capabilities\":[\"execute\"],\"lockable_fields\":[],\"evolves_from\":\"\","
    "\"weight\":5,\"version\":\"v1.0.0\"}";

#define OWNER "owner"
#define COUNT "count"
#define MAX_STEP 1000ULL

typedef unsigned long long u64;

/* ---- tiny string helpers (no libc) ---------------------------------------- */

static int slen(const char *s) { int n = 0; while (s[n]) n++; return n; }

static int same(const char *a, int alen, const char *b, int blen) {
    if (alen != blen) return 0;
    for (int i = 0; i < alen; i++) if (a[i] != b[i]) return 0;
    return 1;
}

/* With -nostdlib the compiler may still emit calls to these two. */
#ifdef __cplusplus
extern "C" {
#endif
void *memcpy(void *d, const void *s, unsigned long n) {
    char *dd = (char *)d; const char *ss = (const char *)s;
    while (n--) *dd++ = *ss++;
    return d;
}
void *memset(void *d, int c, unsigned long n) {
    char *dd = (char *)d;
    while (n--) *dd++ = (char)c;
    return d;
}
#ifdef __cplusplus
}
#endif

/* Offset of needle in hay, or -1. */
static int find(const char *hay, int hlen, const char *needle) {
    int nlen = slen(needle);
    for (int i = 0; i + nlen <= hlen; i++) {
        int j = 0;
        while (j < nlen && hay[i + j] == needle[j]) j++;
        if (j == nlen) return i;
    }
    return -1;
}

/* Length of the JSON string starting at in[start], stopping at its closing quote. */
static int string_end(const char *in, int len, int start) {
    int end = start;
    while (end < len && in[end] != '"') end += (in[end] == '\\') ? 2 : 1;
    return end > len ? -1 : end;
}

/* The string value of "name":"..." in the input. Returns its length, or -1. */
static int field(const char *in, int len, const char *name, const char **out) {
    char pat[40]; int p = 0;
    pat[p++] = '"';
    for (int i = 0; name[i]; i++) pat[p++] = name[i];
    pat[p++] = '"'; pat[p++] = ':'; pat[p++] = '"'; pat[p] = 0;
    int at = find(in, len, pat);
    if (at < 0) return -1;
    int start = at + p, end = string_end(in, len, start);
    if (end < 0) return -1;
    *out = in + start;
    return end - start;
}

/* The first element of "args":[...]. Returns its length, or -1 when there is none. */
static int first_arg(const char *in, int len, const char **out) {
    int at = find(in, len, "\"args\":[");
    if (at < 0) return -1;
    int start = at + 8;
    if (start >= len || in[start] != '"') return -1;
    start++;
    int end = string_end(in, len, start);
    if (end < 0) return -1;
    *out = in + start;
    return end - start;
}

/* Decimal digits only, at most 20 of them. Returns 0 when it is not a number. */
static int parse_u64(const char *s, int n, u64 *v) {
    if (n <= 0 || n > 20) return 0;
    u64 r = 0;
    for (int i = 0; i < n; i++) {
        if (s[i] < '0' || s[i] > '9') return 0;
        r = r * 10 + (u64)(s[i] - '0');
    }
    *v = r;
    return 1;
}

static int fmt_u64(u64 v, char *out) {
    char t[24]; int n = 0;
    do { t[n++] = (char)('0' + v % 10); v /= 10; } while (v);
    for (int i = 0; i < n; i++) out[i] = t[n - 1 - i];
    return n;
}

/* ---- storage and events --------------------------------------------------- */

static int get(const char *key, char *buf, int max) {
    int n = read_state(key, slen(key), buf, max);
    return n < 0 ? 0 : n;
}

static void set(const char *key, const char *val, int val_len) {
    write_state(key, slen(key), val, val_len);
}

static u64 count_now(void) {
    char buf[32]; u64 v = 0;
    int n = get(COUNT, buf, sizeof buf);
    if (n == 0 || !parse_u64(buf, n, &v)) return 0;
    return v;
}

/* Write a JSON string, escaping quotes and backslashes. Returns bytes written. */
static int put_json_string(char *out, const char *s, int n) {
    int w = 0;
    out[w++] = '"';
    for (int i = 0; i < n; i++) {
        if (s[i] == '"' || s[i] == '\\') out[w++] = '\\';
        out[w++] = s[i];
    }
    out[w++] = '"';
    return w;
}

static int put(char *out, const char *s) { int n = slen(s); memcpy(out, s, n); return n; }

struct pair { const char *name; const char *value; int value_len; };

/* Emit an event whose payload is a JSON object built from name/value pairs. */
static void emit(const char *type, const struct pair *pairs, int count) {
    char data[1024]; int w = 0;
    data[w++] = '{';
    for (int i = 0; i < count && w < 900; i++) {
        if (i > 0) data[w++] = ',';
        w += put_json_string(data + w, pairs[i].name, slen(pairs[i].name));
        data[w++] = ':';
        w += put_json_string(data + w, pairs[i].value, pairs[i].value_len);
    }
    data[w++] = '}';
    emit_event(type, slen(type), data, w);
}

/* ---- replies -------------------------------------------------------------- */

static int ok(char *out, const char *data, int n) {
    int w = put(out, "{\"ok\":true,\"data\":");
    w += put_json_string(out + w, data, n);
    out[w++] = '}';
    return w;
}

static int error(char *out, const char *message) {
    int w = put(out, "{\"ok\":false,\"error\":");
    w += put_json_string(out + w, message, slen(message));
    out[w++] = '}';
    return w;
}

/* ---- the entry point ------------------------------------------------------ */

int on_use(const char *in, int len, char *out) {
    const char *action, *caller, *arg;
    int alen = field(in, len, "action", &action);
    int clen = field(in, len, "caller", &caller);
    if (alen < 0 || clen < 0) return error(out, "malformed input");
    char owner[256]; char num[24];

    if (same(action, alen, "init", 4)) {
        /* Record the first caller as the owner. Until this has run, anyone may call it. */
        if (get(OWNER, owner, sizeof owner) > 0) return error(out, "already initialised");
        set(OWNER, caller, clen);
        set(COUNT, "0", 1);
        struct pair p[] = {{"owner", caller, clen}};
        emit("counter_created", p, 1);
        return ok(out, caller, clen);
    }
    if (same(action, alen, "bump", 4)) {
        /* The step is checked before anything is written. */
        u64 step = 1;
        int arglen = first_arg(in, len, &arg);
        if (arglen >= 0 && !parse_u64(arg, arglen, &step)) return error(out, "step must be a number");
        if (step == 0 || step > MAX_STEP) return error(out, "step must be between 1 and 1000");
        u64 before = count_now(), total = before + step;
        if (total < before) return error(out, "counter overflow");
        int n = fmt_u64(total, num);
        set(COUNT, num, n);
        char stepbuf[24]; int sn = fmt_u64(step, stepbuf);
        struct pair p[] = {{"by", caller, clen}, {"step", stepbuf, sn}, {"total", num, n}};
        emit("counter_bumped", p, 3);
        return ok(out, num, n);
    }
    if (same(action, alen, "value", 5)) {
        int n = fmt_u64(count_now(), num);
        return ok(out, num, n);
    }
    if (same(action, alen, "owner", 5)) {
        int n = get(OWNER, owner, sizeof owner);
        return ok(out, owner, n);
    }
    if (same(action, alen, "reset", 5)) {
        /* Only the owner may reset. The caller is proven by the chain. */
        int n = get(OWNER, owner, sizeof owner);
        if (n == 0) return error(out, "not initialised yet");
        if (!same(owner, n, caller, clen)) return error(out, "only the owner may reset this counter");
        set(COUNT, "0", 1);
        struct pair p[] = {{"by", caller, clen}};
        emit("counter_reset", p, 1);
        return ok(out, "0", 1);
    }
    return error(out, "unknown action, try init, bump, value, owner or reset");
}
