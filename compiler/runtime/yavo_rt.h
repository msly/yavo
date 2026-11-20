/* Yavo P0 runtime. Included by every generated C file; requires C17 and a compiler
 * with the GCC/Clang overflow builtins (the pinned `zig cc` provides both). */
#ifndef YAVO_RT_H
#define YAVO_RT_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <stdio.h>

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#else
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>
#endif

#define YV_NORETURN _Noreturn
#define YV_UNREACHABLE() __builtin_unreachable()
#define YV_PANIC_EXIT_CODE 101

typedef struct {
    const char *ptr;
    size_t len;
} yv_str;

/* ---- Output: stdout is buffered, stderr is written immediately. ---- */

static const char *yv_rt_file = "?";
static char yv_out_buf[8192];
static size_t yv_out_len = 0;

static uint32_t yv_io_line = 0, yv_io_col = 0;
static uint32_t yv_out_line = 0, yv_out_col = 0;

YV_NORETURN static void yv_output_failure(uint32_t line, uint32_t col);

static inline void yv_output_at(uint32_t line, uint32_t col) {
    yv_io_line = line;
    yv_io_col = col;
}

/* Raw best-effort writer: never reports an error through the buffered writer. */
static bool yv_os_write(int fd, const char *p, size_t n) {
#ifdef _WIN32
    HANDLE h = GetStdHandle(fd == 2 ? STD_ERROR_HANDLE : STD_OUTPUT_HANDLE);
    while (n > 0) {
        DWORD chunk = n > 0x40000000u ? 0x40000000u : (DWORD)n;
        DWORD written = 0;
        if (!WriteFile(h, p, chunk, &written, NULL) || written == 0) return false;
        p += written;
        n -= written;
    }
#else
    while (n > 0) {
        size_t chunk = n > 0x40000000u ? 0x40000000u : n;
        ssize_t w = write(fd, p, chunk);
        if (w < 0 && errno == EINTR) continue;
        if (w <= 0) return false;
        p += w;
        n -= (size_t)w;
    }
#endif
    return true;
}

static void yv_flush(void) {
    if (yv_out_len > 0) {
        if (!yv_os_write(1, yv_out_buf, yv_out_len))
            yv_output_failure(yv_out_line, yv_out_col);
        yv_out_len = 0;
    }
}

static void yv_write(int fd, const char *p, size_t n) {
    if (fd != 1) {
        yv_flush();
        if (!yv_os_write(fd, p, n)) yv_output_failure(yv_io_line, yv_io_col);
        return;
    }
    if (n > sizeof yv_out_buf - yv_out_len) {
        yv_flush();
        if (n > sizeof yv_out_buf) {
            if (!yv_os_write(1, p, n)) yv_output_failure(yv_io_line, yv_io_col);
            return;
        }
    }
    memcpy(yv_out_buf + yv_out_len, p, n);
    yv_out_len += n;
    if (n > 0) {
        yv_out_line = yv_io_line;
        yv_out_col = yv_io_col;
    }
}

static void yv_write_u64(int fd, uint64_t v) {
    char buf[20];
    size_t i = sizeof buf;
    do {
        buf[--i] = (char)('0' + v % 10);
        v /= 10;
    } while (v != 0);
    yv_write(fd, buf + i, sizeof buf - i);
}

static void yv_write_i64(int fd, int64_t v) {
    if (v < 0) {
        yv_write(fd, "-", 1);
        yv_write_u64(fd, (uint64_t)0 - (uint64_t)v);
    } else {
        yv_write_u64(fd, (uint64_t)v);
    }
}

static void yv_write_bool(int fd, bool b) {
    if (b) yv_write(fd, "true", 4);
    else yv_write(fd, "false", 5);
}

static void yv_write_str(int fd, yv_str s) { yv_write(fd, s.ptr, s.len); }

static bool yv_str_eq(yv_str a, yv_str b) {
    return a.len == b.len && (a.len == 0 || memcmp(a.ptr, b.ptr, a.len) == 0);
}

/* ---- Process lifecycle and panics. ---- */

YV_NORETURN static void yv_exit_process(int code) {
#ifdef _WIN32
    ExitProcess((UINT)code);
#else
    _exit(code);
#endif
    YV_UNREACHABLE();
}

static void yv_os_write_u32(int fd, uint32_t value) {
    char buf[10];
    size_t i = sizeof buf;
    do {
        buf[--i] = (char)('0' + value % 10);
        value /= 10;
    } while (value != 0);
    (void)yv_os_write(fd, buf + i, sizeof buf - i);
}

/* stdout/stderr may both be unusable. Attempt one raw diagnostic, then exit;
 * do not call yv_panic/yv_write/yv_flush here and recurse on the same error. */
YV_NORETURN static void yv_output_failure(uint32_t line, uint32_t col) {
    static const char prefix[] = "yavo panic [P-SYSTEM] at ";
    static const char suffix[] = ": output write failed\n";
    (void)yv_os_write(2, prefix, sizeof prefix - 1);
    (void)yv_os_write(2, yv_rt_file, strlen(yv_rt_file));
    (void)yv_os_write(2, ":", 1);
    yv_os_write_u32(2, line);
    (void)yv_os_write(2, ":", 1);
    yv_os_write_u32(2, col);
    (void)yv_os_write(2, suffix, sizeof suffix - 1);
    yv_exit_process(YV_PANIC_EXIT_CODE);
}

static void yv_rt_init(const char *file) { yv_rt_file = file; }

/* Test builds (`YAVO_ALLOC_CHECK=1` adds -DYV_ALLOC_CHECK) count live heap blocks. Freeing more
 * blocks than were allocated (two copies of one value both dropped) stops the program at once;
 * blocks still alive at a normal exit (a missed drop) are reported. Both exit with status 102.
 * Use-after-free is not detected by this check. */
#ifdef YV_ALLOC_CHECK
static int64_t yv_live_blocks = 0;
#define YV_COUNT_ALLOC() ((void)__atomic_add_fetch(&yv_live_blocks, 1, __ATOMIC_RELAXED))
#define YV_COUNT_FREE()                                                                   \
    do {                                                                                  \
        if (__atomic_sub_fetch(&yv_live_blocks, 1, __ATOMIC_RELAXED) < 0) {                                                    \
            yv_flush();                                                                   \
            yv_write(2, "yavo alloc-check: a heap block was freed twice\n", 47);          \
            yv_exit_process(102);                                                         \
        }                                                                                 \
    } while (0)
#else
#define YV_COUNT_ALLOC() ((void)0)
#define YV_COUNT_FREE() ((void)0)
#endif

static int yv_rt_exit(void) {
    yv_flush();
#ifdef YV_ALLOC_CHECK
    if (yv_live_blocks != 0) {
        yv_write(2, "yavo alloc-check: ", 18);
        yv_write_i64(2, yv_live_blocks);
        yv_write(2, " heap block(s) still alive at exit\n", 35);
        return 102;
    }
#endif
    return 0;
}

/* Format fixed by ERRORS.md: `yavo panic [<category>] at <file>:<line>:<col>: <message>` */
static void yv_panic_begin(const char *category, uint32_t line, uint32_t col) {
    yv_flush();
    yv_write(2, "yavo panic [", 12);
    yv_write(2, category, strlen(category));
    yv_write(2, "] at ", 5);
    yv_write(2, yv_rt_file, strlen(yv_rt_file));
    yv_write(2, ":", 1);
    yv_write_u64(2, line);
    yv_write(2, ":", 1);
    yv_write_u64(2, col);
    yv_write(2, ": ", 2);
}

YV_NORETURN static void yv_panic_end(void) {
    yv_write(2, "\n", 1);
    yv_exit_process(YV_PANIC_EXIT_CODE);
    YV_UNREACHABLE();
}

YV_NORETURN static void yv_panic(const char *category, const char *message, uint32_t line, uint32_t col) {
    yv_panic_begin(category, line, col);
    yv_write(2, message, strlen(message));
    yv_panic_end();
}

/* ---- Integer operations, instantiated for every P0 integer type. ---- */

#define YV_INT_OPS(S, T, BITS, SIGNED, MIN)                                                         \
    static inline T yv_add_##S(T a, T b, uint32_t line, uint32_t col) {                             \
        T r;                                                                                        \
        if (__builtin_add_overflow(a, b, &r)) yv_panic("P-OVERFLOW", "integer overflow in `+` on " #S, line, col); \
        return r;                                                                                   \
    }                                                                                               \
    static inline T yv_sub_##S(T a, T b, uint32_t line, uint32_t col) {                             \
        T r;                                                                                        \
        if (__builtin_sub_overflow(a, b, &r)) yv_panic("P-OVERFLOW", "integer overflow in `-` on " #S, line, col); \
        return r;                                                                                   \
    }                                                                                               \
    static inline T yv_mul_##S(T a, T b, uint32_t line, uint32_t col) {                             \
        T r;                                                                                        \
        if (__builtin_mul_overflow(a, b, &r)) yv_panic("P-OVERFLOW", "integer overflow in `*` on " #S, line, col); \
        return r;                                                                                   \
    }                                                                                               \
    static inline T yv_neg_##S(T a, uint32_t line, uint32_t col) {                                  \
        if (SIGNED && a == (T)(MIN)) yv_panic("P-OVERFLOW", "integer overflow in unary `-` on " #S, line, col); \
        return (T)(0 - a);                                                                          \
    }                                                                                               \
    static inline T yv_div_##S(T a, T b, uint32_t line, uint32_t col) {                             \
        if (b == 0) yv_panic("P-DIV-ZERO", "division by zero", line, col);                         \
        if (SIGNED && a == (T)(MIN) && b == (T)-1)                                                  \
            yv_panic("P-DIV-OVERFLOW", "division of the minimum " #S " by -1 overflows", line, col);      \
        return (T)(a / b);                                                                          \
    }                                                                                               \
    static inline T yv_rem_##S(T a, T b, uint32_t line, uint32_t col) {                             \
        if (b == 0) yv_panic("P-DIV-ZERO", "remainder by zero", line, col);                        \
        if (SIGNED && a == (T)(MIN) && b == (T)-1)                                                  \
            yv_panic("P-DIV-OVERFLOW", "remainder of the minimum " #S " by -1 overflows", line, col);     \
        return (T)(a % b);                                                                          \
    }                                                                                               \
    static inline T yv_addw_##S(T a, T b) { return (T)((uint64_t)a + (uint64_t)b); }                \
    static inline T yv_subw_##S(T a, T b) { return (T)((uint64_t)a - (uint64_t)b); }                \
    static inline T yv_mulw_##S(T a, T b) { return (T)((uint64_t)a * (uint64_t)b); }                \
    static inline T yv_shl_##S(T a, int64_t n, uint32_t line, uint32_t col) {                       \
        if (n < 0 || n >= BITS) yv_panic("P-SHIFT", "shift amount out of range for " #S, line, col); \
        return (T)((uint64_t)a << n);                                                               \
    }                                                                                               \
    static inline T yv_shr_##S(T a, int64_t n, uint32_t line, uint32_t col) {                       \
        if (n < 0 || n >= BITS) yv_panic("P-SHIFT", "shift amount out of range for " #S, line, col); \
        return (T)(a >> n);                                                                         \
    }

YV_INT_OPS(i8, int8_t, 8, 1, INT8_MIN)
YV_INT_OPS(i16, int16_t, 16, 1, INT16_MIN)
YV_INT_OPS(i32, int32_t, 32, 1, INT32_MIN)
YV_INT_OPS(i64, int64_t, 64, 1, INT64_MIN)
YV_INT_OPS(u8, uint8_t, 8, 0, 0)
YV_INT_OPS(u16, uint16_t, 16, 0, 0)
YV_INT_OPS(u32, uint32_t, 32, 0, 0)
YV_INT_OPS(u64, uint64_t, 64, 0, 0)

static inline int64_t yv_shift_amount_u64(uint64_t n, uint32_t line, uint32_t col) {
    if (n > 63) yv_panic("P-SHIFT", "shift amount out of range", line, col);
    return (int64_t)n;
}

/* ---- Checked conversions `T(x)`: panic with P-CONVERT when the value does not fit. ---- */

static inline void yv_conv_check_s(int64_t v, int64_t lo, uint64_t hi, uint32_t line, uint32_t col) {
    if (v < lo || (v > 0 && (uint64_t)v > hi))
        yv_panic("P-CONVERT", "integer conversion out of range", line, col);
}

static inline void yv_conv_check_u(uint64_t v, uint64_t hi, uint32_t line, uint32_t col) {
    if (v > hi) yv_panic("P-CONVERT", "integer conversion out of range", line, col);
}

#define YV_CONV_S(FS, FT, TS, TT, LO, HI)                                       \
    static inline TT yv_conv_##FS##_##TS(FT v, uint32_t line, uint32_t col) {   \
        yv_conv_check_s((int64_t)v, LO, HI, line, col);                         \
        return (TT)v;                                                           \
    }
#define YV_CONV_U(FS, FT, TS, TT, LO, HI)                                       \
    static inline TT yv_conv_##FS##_##TS(FT v, uint32_t line, uint32_t col) {   \
        yv_conv_check_u((uint64_t)v, HI, line, col);                            \
        return (TT)v;                                                           \
    }
#define YV_CONV_TO_ALL(KIND, FS, FT)                                             \
    KIND(FS, FT, i8, int8_t, INT8_MIN, INT8_MAX)                                 \
    KIND(FS, FT, i16, int16_t, INT16_MIN, INT16_MAX)                             \
    KIND(FS, FT, i32, int32_t, INT32_MIN, INT32_MAX)                             \
    KIND(FS, FT, i64, int64_t, INT64_MIN, INT64_MAX)                             \
    KIND(FS, FT, u8, uint8_t, 0, UINT8_MAX)                                      \
    KIND(FS, FT, u16, uint16_t, 0, UINT16_MAX)                                   \
    KIND(FS, FT, u32, uint32_t, 0, UINT32_MAX)                                   \
    KIND(FS, FT, u64, uint64_t, 0, UINT64_MAX)

YV_CONV_TO_ALL(YV_CONV_S, i8, int8_t)
YV_CONV_TO_ALL(YV_CONV_S, i16, int16_t)
YV_CONV_TO_ALL(YV_CONV_S, i32, int32_t)
YV_CONV_TO_ALL(YV_CONV_S, i64, int64_t)
YV_CONV_TO_ALL(YV_CONV_U, u8, uint8_t)
YV_CONV_TO_ALL(YV_CONV_U, u16, uint16_t)
YV_CONV_TO_ALL(YV_CONV_U, u32, uint32_t)
YV_CONV_TO_ALL(YV_CONV_U, u64, uint64_t)

/* ---- Owned heap values: String and Vec. ----
 * A moved-from or empty value is all zeros, so dropping it is a no-op. Growth checks the
 * capacity arithmetic (P-CAPACITY) and the allocation itself (P-OOM). */

typedef struct {
    char *ptr;
    int64_t len;
    int64_t cap;
} yv_string;

typedef struct {
    void *ptr;
    int64_t len;
    int64_t cap;
} yv_vec;

YV_NORETURN static void yv_capacity_overflow(uint32_t line, uint32_t col) {
    yv_panic("P-CAPACITY", "capacity overflow", line, col);
}

/* Makes room for at least `need` elements of `elem` bytes, growing geometrically. */
static void yv_grow(void **ptr, int64_t *cap, int64_t elem, int64_t need, uint32_t line, uint32_t col) {
    if (need <= *cap) return;
    int64_t newcap = *cap < 4 ? 4 : *cap;
    while (newcap < need) {
        if (__builtin_mul_overflow(newcap, (int64_t)2, &newcap)) {
            newcap = need;
            break;
        }
    }
    size_t bytes;
    if (__builtin_mul_overflow((size_t)newcap, (size_t)elem, &bytes) || bytes > (size_t)PTRDIFF_MAX) {
        yv_capacity_overflow(line, col);
    }
    bool fresh = *ptr == NULL;
    void *p = realloc(*ptr, bytes);
    if (p == NULL) yv_panic("P-OOM", "out of memory", line, col);
    if (fresh) YV_COUNT_ALLOC();
    *ptr = p;
    *cap = newcap;
}

static inline yv_str yv_string_view(const yv_string *s) {
    yv_str v = {s->ptr ? s->ptr : "", (size_t)s->len};
    return v;
}

static void yv_string_push(yv_string *s, yv_str t, uint32_t line, uint32_t col) {
    int64_t need;
    if (t.len > (size_t)INT64_MAX || __builtin_add_overflow(s->len, (int64_t)t.len, &need)) {
        yv_capacity_overflow(line, col);
    }
    yv_grow((void **)&s->ptr, &s->cap, 1, need, line, col);
    if (t.len) memcpy(s->ptr + s->len, t.ptr, t.len);
    s->len = need;
}

static yv_string yv_string_from(yv_str t, uint32_t line, uint32_t col) {
    yv_string s = {0};
    yv_string_push(&s, t, line, col);
    return s;
}

static void yv_string_drop(yv_string *s) {
    if (s->ptr) {
        YV_COUNT_FREE();
        free(s->ptr);
    }
    s->ptr = NULL;
    s->len = s->cap = 0;
}

static int yv_str_cmp(yv_str a, yv_str b) {
    size_t n = a.len < b.len ? a.len : b.len;
    int c = n ? memcmp(a.ptr, b.ptr, n) : 0;
    if (c != 0) return c < 0 ? -1 : 1;
    return a.len < b.len ? -1 : (a.len > b.len ? 1 : 0);
}

/* `s[i]`: the byte at index `i`. */
static inline uint8_t yv_str_at(yv_str s, int64_t i, uint32_t line, uint32_t col) {
    if (i < 0 || (uint64_t)i >= (uint64_t)s.len) yv_panic("P-INDEX", "index out of bounds", line, col);
    return (uint8_t)s.ptr[i];
}

/* Appends one element slot and returns its address; the caller stores the value. */
static void *yv_vec_push_slot(yv_vec *v, int64_t elem, uint32_t line, uint32_t col) {
    int64_t need;
    if (__builtin_add_overflow(v->len, (int64_t)1, &need)) yv_capacity_overflow(line, col);
    yv_grow(&v->ptr, &v->cap, elem, need, line, col);
    void *slot = (char *)v->ptr + v->len * elem;
    v->len = need;
    return slot;
}

static void *yv_vec_at(const yv_vec *v, int64_t elem, int64_t i, uint32_t line, uint32_t col) {
    if (i < 0 || i >= v->len) yv_panic("P-INDEX", "index out of bounds", line, col);
    return (char *)v->ptr + i * elem;
}

/* `v.reserve(n)`: total capacity at least `n`; a negative `n` or an impossible size panics. */
static void yv_vec_reserve(yv_vec *v, int64_t elem, int64_t n, uint32_t line, uint32_t col) {
    if (n < 0) yv_capacity_overflow(line, col);
    yv_grow(&v->ptr, &v->cap, elem, n, line, col);
}

/* `v[a..b]`: a view of elements `a` up to (not including) `b`; it owns nothing. */
static yv_vec yv_vec_slice(yv_vec v, int64_t a, int64_t b, int64_t elem, uint32_t line, uint32_t col) {
    if (a < 0 || b < a || b > v.len) yv_panic("P-INDEX", "slice bounds out of range", line, col);
    yv_vec s = {v.ptr ? (char *)v.ptr + a * elem : NULL, b - a, b - a};
    return s;
}

/* `s[a..b]` on text: bytes `a` up to `b`; both ends must fall on character boundaries (S3). */
static yv_str yv_str_slice(yv_str s, int64_t a, int64_t b, uint32_t line, uint32_t col) {
    if (a < 0 || b < a || (uint64_t)b > (uint64_t)s.len) yv_panic("P-INDEX", "slice bounds out of range", line, col);
    if ((a < (int64_t)s.len && ((unsigned char)s.ptr[a] & 0xC0) == 0x80) ||
        (b < (int64_t)s.len && ((unsigned char)s.ptr[b] & 0xC0) == 0x80)) {
        yv_panic("P-STR-BOUNDARY", "a text slice must start and end on character boundaries", line, col);
    }
    yv_str r = {s.ptr + a, (size_t)(b - a)};
    return r;
}

/* An empty vector with room for exactly `n` elements (vector literals, clones). */
static yv_vec yv_vec_with_cap(int64_t elem, int64_t n, uint32_t line, uint32_t col) {
    yv_vec v = {0};
    if (n > 0) {
        v.cap = 0;
        yv_grow(&v.ptr, &v.cap, elem, n, line, col);
    }
    return v;
}

static void yv_vec_free(yv_vec *v) {
    if (v->ptr) {
        YV_COUNT_FREE();
        free(v->ptr);
    }
    v->ptr = NULL;
    v->len = v->cap = 0;
}

/* ---- Box[T]: one heap block; NULL is an empty (moved-out) box. ---- */
static void *yv_box_alloc(size_t size, uint32_t line, uint32_t col) {
    void *p = malloc(size ? size : 1);
    if (p == NULL) yv_panic("P-OOM", "out of memory", line, col);
    YV_COUNT_ALLOC();
    return p;
}

static void yv_box_free(void *p) {
    if (p) {
        YV_COUNT_FREE();
        free(p);
    }
}

/* ---- Map[K, V]: insertion-ordered hash map. ----
 * Entries live in `ents` in insertion order; the generated per-type entry struct starts with a
 * `bool live` flag. A removed entry stays (not live) until the next rebuild and keeps its slot,
 * so lookups probe past it. `slots` is an open-addressing table of entry index + 1 (0 = empty)
 * whose size is a power of two kept at least twice the entry count, so probing always ends.
 * All-zero is an empty map. The generated code supplies hashing and key comparison. */

typedef struct {
    yv_vec ents;
    int64_t live;
    int64_t *slots;
    int64_t nslots;
} yv_map;

/* splitmix64 finalizer */
static inline uint64_t yv_hash_u64(uint64_t x) {
    x ^= x >> 30;
    x *= 0xbf58476d1ce4e5b9ULL;
    x ^= x >> 27;
    x *= 0x94d049bb133111ebULL;
    x ^= x >> 31;
    return x;
}

/* FNV-1a, finalized; `String` and `str` keys hash their UTF-8 bytes the same way. */
static inline uint64_t yv_hash_bytes(const uint8_t *p, size_t n) {
    uint64_t h = 0xcbf29ce484222325ULL;
    for (size_t i = 0; i < n; i++) {
        h ^= p[i];
        h *= 0x100000001b3ULL;
    }
    return yv_hash_u64(h);
}

/* The slot table size for `entries` entries: a power of two, at least 4 * entries + 4. */
static int64_t yv_map_slot_count(int64_t entries, uint32_t line, uint32_t col) {
    int64_t want, n = 8;
    if (__builtin_mul_overflow(entries, (int64_t)4, &want) || __builtin_add_overflow(want, (int64_t)4, &want)) {
        yv_capacity_overflow(line, col);
    }
    while (n < want) {
        if (__builtin_mul_overflow(n, (int64_t)2, &n)) yv_capacity_overflow(line, col);
    }
    return n;
}

/* Replaces the slot table with an empty one of `n` slots. */
static void yv_map_slots_reset(yv_map *m, int64_t n, uint32_t line, uint32_t col) {
    if (m->slots) {
        YV_COUNT_FREE();
        free(m->slots);
        m->slots = NULL;
        m->nslots = 0;
    }
    if ((uint64_t)n > (uint64_t)(PTRDIFF_MAX / (int64_t)sizeof(int64_t))) yv_capacity_overflow(line, col);
    int64_t *s = (int64_t *)calloc((size_t)n, sizeof(int64_t));
    if (s == NULL) yv_panic("P-OOM", "out of memory", line, col);
    YV_COUNT_ALLOC();
    m->slots = s;
    m->nslots = n;
}

/* Records entry `idx` (whose key hashes to `h`) in the first free slot of its probe sequence. */
static void yv_map_slot_put(yv_map *m, uint64_t h, int64_t idx) {
    uint64_t mask = (uint64_t)m->nslots - 1, i = h & mask;
    while (m->slots[i] != 0) i = (i + 1) & mask;
    m->slots[i] = idx + 1;
}

/* Frees the storage; the entries' contents must already be dropped. */
static void yv_map_free(yv_map *m) {
    yv_vec_free(&m->ents);
    if (m->slots) {
        YV_COUNT_FREE();
        free(m->slots);
    }
    m->slots = NULL;
    m->nslots = 0;
    m->live = 0;
}

/* `os.exit(code)`: output is flushed; nothing else is cleaned up (V11). */
YV_NORETURN static void yv_os_exit(int64_t code) {
    yv_flush();
    yv_exit_process((int)code);
    YV_UNREACHABLE();
}

/* ---- Views into text. The results point into the original bytes; nothing is copied. ---- */

static inline bool yv_is_space(char c) { return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\v' || c == '\f'; }

/* ASCII whitespace removed from both ends. */
static yv_str yv_str_trim(yv_str s) {
    size_t a = 0, b = s.len;
    while (a < b && yv_is_space(s.ptr[a])) a++;
    while (b > a && yv_is_space(s.ptr[b - 1])) b--;
    yv_str r = {s.ptr + a, b - a};
    return r;
}

static void yv_push_view(yv_vec *v, const char *p, size_t n, uint32_t line, uint32_t col) {
    yv_str piece = {p, n};
    *(yv_str *)yv_vec_push_slot(v, (int64_t)sizeof(yv_str), line, col) = piece;
}

/* Maximal runs of non-whitespace. */
static yv_vec yv_str_fields(yv_str s, uint32_t line, uint32_t col) {
    yv_vec v = {0};
    size_t i = 0;
    while (i < s.len) {
        while (i < s.len && yv_is_space(s.ptr[i])) i++;
        size_t start = i;
        while (i < s.len && !yv_is_space(s.ptr[i])) i++;
        if (i > start) yv_push_view(&v, s.ptr + start, i - start, line, col);
    }
    return v;
}

/* Lines without their `\n` (and a `\r` before it); a final newline does not start a new line. */
static yv_vec yv_str_lines(yv_str s, uint32_t line, uint32_t col) {
    yv_vec v = {0};
    size_t start = 0;
    for (size_t i = 0; i < s.len; i++) {
        if (s.ptr[i] == '\n') {
            size_t end = (i > start && s.ptr[i - 1] == '\r') ? i - 1 : i;
            yv_push_view(&v, s.ptr + start, end - start, line, col);
            start = i + 1;
        }
    }
    if (start < s.len) yv_push_view(&v, s.ptr + start, s.len - start, line, col);
    return v;
}

/* ---- Output targets: printing writes to a file descriptor, `format` appends to a String. ---- */

typedef struct {
    int fd;
    yv_string *buf;
    uint32_t line, col;
} yv_out;

static void yv_out_bytes(const yv_out *o, const char *p, size_t n) {
    if (o->buf) {
        yv_str t = {p, n};
        yv_string_push(o->buf, t, o->line, o->col);
    } else {
        yv_write(o->fd, p, n);
    }
}

static void yv_out_u64(const yv_out *o, uint64_t v) {
    char buf[20];
    size_t i = sizeof buf;
    do {
        buf[--i] = (char)('0' + v % 10);
        v /= 10;
    } while (v != 0);
    yv_out_bytes(o, buf + i, sizeof buf - i);
}

static void yv_out_i64(const yv_out *o, int64_t v) {
    if (v < 0) {
        yv_out_bytes(o, "-", 1);
        yv_out_u64(o, (uint64_t)0 - (uint64_t)v);
    } else {
        yv_out_u64(o, (uint64_t)v);
    }
}

static void yv_out_bool(const yv_out *o, bool b) {
    if (b) yv_out_bytes(o, "true", 4);
    else yv_out_bytes(o, "false", 5);
}

static void yv_out_str(const yv_out *o, yv_str s) { yv_out_bytes(o, s.ptr, s.len); }

/* Floats print as the shortest decimal that reads back as the same value, always with a `.`
 * or an exponent so they cannot be mistaken for integers. Exponents from -7 to 20 print in
 * fixed notation (10.0, 0.001, 1e-7 is 0.0000001), others as `1e21`, `1.5e-8`; also -0.0,
 * inf, -inf, NaN. */
static void yv_out_float(const yv_out *o, double v, bool single) {
    if (v != v) {
        yv_out_bytes(o, "NaN", 3);
        return;
    }
    if (isinf(v)) {
        if (v < 0) yv_out_bytes(o, "-inf", 4);
        else yv_out_bytes(o, "inf", 3);
        return;
    }
    char sci[48], buf[400];
    int p = 1;
    for (; p <= 17; p++) {
        snprintf(sci, sizeof sci, "%.*e", p - 1, v);
        if (single ? strtof(sci, NULL) == (float)v : strtod(sci, NULL) == v) break;
    }
    char *e = strchr(sci, 'e');
    int exp = e ? atoi(e + 1) : 0;
    int n;
    if (exp >= -7 && exp < 21) {
        int decimals = p - 1 - exp;
        n = snprintf(buf, sizeof buf, "%.*f", decimals > 0 ? decimals : 0, v);
        if (n < 0) n = 0;
        yv_out_bytes(o, buf, (size_t)n);
        if (strchr(buf, '.') == NULL) yv_out_bytes(o, ".0", 2);
        return;
    }
    /* `1.5e-08` becomes `1.5e-8`. */
    n = e ? (int)(e - sci) : (int)strlen(sci);
    yv_out_bytes(o, sci, (size_t)n);
    n = snprintf(buf, sizeof buf, "e%d", exp);
    yv_out_bytes(o, buf, (size_t)(n > 0 ? n : 0));
}

static void yv_out_f64(const yv_out *o, double v) { yv_out_float(o, v, false); }
static void yv_out_f32(const yv_out *o, float v) { yv_out_float(o, (double)v, true); }

/* `${x:.N}`: exactly N digits after the point (rounded as by C `%.*f`); NaN and infinities
 * print as in `yv_out_float`. */
static void yv_out_fixed(const yv_out *o, double v, int digits) {
    if (v != v || isinf(v)) {
        yv_out_float(o, v, false);
        return;
    }
    char buf[400];
    int n = snprintf(buf, sizeof buf, "%.*f", digits, v);
    yv_out_bytes(o, buf, (size_t)(n > 0 ? n : 0));
}

/* `totalCmp` (stdlib contract §4): a total order over all bit patterns, from flipped-sign
 * unsigned keys. The result is an `Ordering` tag: Less 0, Equal 1, Greater 2. */
static inline uint32_t yv_f64_total_cmp(double a, double b) {
    uint64_t x, y;
    memcpy(&x, &a, sizeof x);
    memcpy(&y, &b, sizeof y);
    x = (x >> 63) ? ~x : (x | 0x8000000000000000ull);
    y = (y >> 63) ? ~y : (y | 0x8000000000000000ull);
    return x < y ? 0u : (x == y ? 1u : 2u);
}

static inline uint32_t yv_f32_total_cmp(float a, float b) {
    uint32_t x, y;
    memcpy(&x, &a, sizeof x);
    memcpy(&y, &b, sizeof y);
    x = (x >> 31) ? ~x : (x | 0x80000000u);
    y = (y >> 31) ? ~y : (y | 0x80000000u);
    return x < y ? 0u : (x == y ? 1u : 2u);
}

/* Float to integer: truncates toward zero; NaN, infinities and values outside the target's
 * range panic (semantic table §7). The bounds are exclusive and exactly representable. */
#define YV_F2I(S, T, LOW_OK, HIGH)                                                             \
    static inline T yv_f2i_##S(double x, uint32_t line, uint32_t col) {                        \
        if (!((LOW_OK) && x < (HIGH)))                                                         \
            yv_panic("P-FLOAT-CONVERT", "float value is NaN, infinite or out of range for " #S, line, col); \
        return (T)x;                                                                           \
    }
YV_F2I(i8, int8_t, x > -129.0, 128.0)
YV_F2I(i16, int16_t, x > -32769.0, 32768.0)
YV_F2I(i32, int32_t, x > -2147483649.0, 2147483648.0)
YV_F2I(i64, int64_t, x >= -9223372036854775808.0, 9223372036854775808.0)
YV_F2I(u8, uint8_t, x > -1.0, 256.0)
YV_F2I(u16, uint16_t, x > -1.0, 65536.0)
YV_F2I(u32, uint32_t, x > -1.0, 4294967296.0)
YV_F2I(u64, uint64_t, x > -1.0, 18446744073709551616.0)

/* ---- Text queries and conversions (stdlib contract §2). ---- */

/* Byte offset of the first occurrence of `t` in `s`, or -1. */
static int64_t yv_str_find(yv_str s, yv_str t) {
    if (t.len == 0) return 0;
    if (t.len > s.len) return -1;
    for (size_t i = 0; i + t.len <= s.len; i++) {
        if (s.ptr[i] == t.ptr[0] && memcmp(s.ptr + i, t.ptr, t.len) == 0) return (int64_t)i;
    }
    return -1;
}

static bool yv_str_starts(yv_str s, yv_str t) { return t.len <= s.len && (t.len == 0 || memcmp(s.ptr, t.ptr, t.len) == 0); }

static bool yv_str_ends(yv_str s, yv_str t) {
    return t.len <= s.len && (t.len == 0 || memcmp(s.ptr + s.len - t.len, t.ptr, t.len) == 0);
}

/* Pieces between occurrences of `sep` (empty pieces kept); an empty `sep` gives the whole text. */
static yv_vec yv_str_split(yv_str s, yv_str sep, uint32_t line, uint32_t col) {
    yv_vec v = {0};
    if (sep.len == 0) {
        yv_push_view(&v, s.ptr, s.len, line, col);
        return v;
    }
    size_t start = 0, i = 0;
    while (i + sep.len <= s.len) {
        if (memcmp(s.ptr + i, sep.ptr, sep.len) == 0) {
            yv_push_view(&v, s.ptr + start, i - start, line, col);
            i += sep.len;
            start = i;
        } else {
            i++;
        }
    }
    yv_push_view(&v, s.ptr + start, s.len - start, line, col);
    return v;
}

/* ASCII letters changed; every other byte (including non-ASCII UTF-8) is kept as it is. */
static yv_string yv_str_case(yv_str s, bool upper, uint32_t line, uint32_t col) {
    yv_string r = yv_string_from(s, line, col);
    for (int64_t i = 0; i < r.len; i++) {
        char c = r.ptr[i];
        if (upper && c >= 'a' && c <= 'z') r.ptr[i] = (char)(c - 32);
        if (!upper && c >= 'A' && c <= 'Z') r.ptr[i] = (char)(c + 32);
    }
    return r;
}

/* An optional sign followed by decimal digits, nothing else; false when out of `int` range. */
static bool yv_str_parse_int(yv_str s, int64_t *out) {
    size_t i = 0;
    bool neg = false;
    if (i < s.len && (s.ptr[i] == '+' || s.ptr[i] == '-')) {
        neg = s.ptr[i] == '-';
        i++;
    }
    if (i == s.len) return false;
    uint64_t v = 0, limit = neg ? (uint64_t)INT64_MAX + 1 : (uint64_t)INT64_MAX;
    for (; i < s.len; i++) {
        char c = s.ptr[i];
        if (c < '0' || c > '9') return false;
        uint64_t d = (uint64_t)(c - '0');
        if (v > (limit - d) / 10) return false;
        v = v * 10 + d;
    }
    *out = neg ? (int64_t)((uint64_t)0 - v) : (int64_t)v;
    return true;
}

/* `s.parseFloat()`: decimal notation only (digits, `.`, sign, exponent), the whole text. */
static bool yv_str_parse_float(yv_str s, double *out) {
    char buf[400];
    if (s.len == 0 || s.len >= sizeof buf) return false;
    for (size_t i = 0; i < s.len; i++) {
        char c = s.ptr[i];
        if (!((c >= '0' && c <= '9') || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')) return false;
        buf[i] = c;
    }
    buf[s.len] = 0;
    char *end = NULL;
    double v = strtod(buf, &end);
    if (end != buf + s.len) return false;
    *out = v;
    return true;
}

/* -1 for valid UTF-8, otherwise the offset of the first invalid (or truncated) sequence. */
static int64_t yv_utf8_check(const char *p, size_t n) {
    const unsigned char *s = (const unsigned char *)p;
    size_t i = 0;
    while (i < n) {
        unsigned char c = s[i];
        size_t len;
        uint32_t min, cp;
        if (c < 0x80) {
            i++;
            continue;
        } else if ((c & 0xE0) == 0xC0) {
            len = 2, min = 0x80, cp = c & 0x1F;
        } else if ((c & 0xF0) == 0xE0) {
            len = 3, min = 0x800, cp = c & 0x0F;
        } else if ((c & 0xF8) == 0xF0) {
            len = 4, min = 0x10000, cp = c & 0x07;
        } else {
            return (int64_t)i;
        }
        if (i + len > n) return (int64_t)i;
        for (size_t k = 1; k < len; k++) {
            if ((s[i + k] & 0xC0) != 0x80) return (int64_t)i;
            cp = (cp << 6) | (s[i + k] & 0x3F);
        }
        if (cp < min || cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF)) return (int64_t)i;
        i += len;
    }
    return -1;
}

/* ---- Stable sorting (stdlib contract §4): merge sort with a temporary buffer. ---- */

typedef int (*yv_cmp_fn)(const void *, const void *);
/* A comparison with a context (the captured values of a `sortBy` closure). */
typedef int (*yv_cmp_ctx_fn)(void *, const void *, const void *);

/* Only `cmp(x, y) < 0` ("x goes strictly before y") is asked, so a comparison that breaks its
   contract gives a wrong order but never touches memory outside the vector (K3). */
static void yv_sort_ctx(yv_vec *v, size_t elem, yv_cmp_ctx_fn cmp, void *ctx, uint32_t line, uint32_t col) {
    int64_t n = v->len;
    if (n < 2) return;
    size_t bytes;
    if (__builtin_mul_overflow((size_t)n, elem, &bytes)) yv_capacity_overflow(line, col);
    char *tmp = (char *)malloc(bytes);
    if (tmp == NULL) yv_panic("P-OOM", "out of memory", line, col);
    YV_COUNT_ALLOC();
    char *a = (char *)v->ptr, *src = a, *dst = tmp;
    for (int64_t width = 1; width < n; width *= 2) {
        for (int64_t lo = 0; lo < n; lo += 2 * width) {
            int64_t mid = lo + width < n ? lo + width : n, hi = lo + 2 * width < n ? lo + 2 * width : n;
            int64_t i = lo, j = mid, k = lo;
            while (i < mid && j < hi) {
                /* Ties keep the left element first: the sort is stable. */
                if (cmp(ctx, src + (size_t)j * elem, src + (size_t)i * elem) < 0) memcpy(dst + (size_t)k++ * elem, src + (size_t)j++ * elem, elem);
                else memcpy(dst + (size_t)k++ * elem, src + (size_t)i++ * elem, elem);
            }
            while (i < mid) memcpy(dst + (size_t)k++ * elem, src + (size_t)i++ * elem, elem);
            while (j < hi) memcpy(dst + (size_t)k++ * elem, src + (size_t)j++ * elem, elem);
        }
        char *t = src;
        src = dst;
        dst = t;
    }
    if (src != a) memcpy(a, src, bytes);
    YV_COUNT_FREE();
    free(tmp);
}

typedef struct { yv_cmp_fn cmp; } yv_cmp_box;

static int yv_cmp_unboxed(void *ctx, const void *a, const void *b) {
    return ((yv_cmp_box *)ctx)->cmp(a, b);
}

static void yv_sort(yv_vec *v, size_t elem, yv_cmp_fn cmp, uint32_t line, uint32_t col) {
    yv_cmp_box box = {cmp};
    yv_sort_ctx(v, elem, yv_cmp_unboxed, &box, line, col);
}

#define YV_CMP_INT(S, T)                                          \
    static int yv_cmp_##S(const void *a, const void *b) {         \
        T x = *(const T *)a, y = *(const T *)b;                   \
        return x < y ? -1 : (x > y ? 1 : 0);                      \
    }
YV_CMP_INT(i8, int8_t)
YV_CMP_INT(i16, int16_t)
YV_CMP_INT(i32, int32_t)
YV_CMP_INT(i64, int64_t)
YV_CMP_INT(u8, uint8_t)
YV_CMP_INT(u16, uint16_t)
YV_CMP_INT(u32, uint32_t)
YV_CMP_INT(u64, uint64_t)
static int yv_cmp_str(const void *a, const void *b) { return yv_str_cmp(*(const yv_str *)a, *(const yv_str *)b); }
static int yv_cmp_String(const void *a, const void *b) {
    return yv_str_cmp(yv_string_view((const yv_string *)a), yv_string_view((const yv_string *)b));
}

/* ---- Standard input, files and command-line arguments (stdlib contract §5). ----
 * The I/O functions return 0 on success, 1 for invalid UTF-8 (`*code` = byte offset), 2 for a
 * system error (`*code` = errno or GetLastError()) and 3 for a path containing NUL; the
 * generated code turns that into the `io.Error` / `fs.Error` case with the same number - 1. */

static char yv_in_buf[65536];
static size_t yv_in_pos = 0, yv_in_len = 0;
static bool yv_in_eof = false;

/* Refills the stdin buffer; false at end of input. A system error is reported through `*err`. */
static bool yv_in_fill(int64_t *err) {
    if (yv_in_eof) return false;
    yv_flush(); /* a prompt written before reading must be visible */
#ifdef _WIN32
    DWORD got = 0;
    if (!ReadFile(GetStdHandle(STD_INPUT_HANDLE), yv_in_buf, (DWORD)sizeof yv_in_buf, &got, NULL)) {
        DWORD e = GetLastError();
        if (e == ERROR_BROKEN_PIPE) {
            yv_in_eof = true;
            return false;
        }
        *err = (int64_t)e;
        return false;
    }
    if (got == 0) {
        yv_in_eof = true;
        return false;
    }
    yv_in_pos = 0;
    yv_in_len = got;
    return true;
#else
    for (;;) {
        ssize_t got = read(0, yv_in_buf, sizeof yv_in_buf);
        if (got < 0 && errno == EINTR) continue;
        if (got < 0) {
            *err = errno;
            return false;
        }
        if (got == 0) {
            yv_in_eof = true;
            return false;
        }
        yv_in_pos = 0;
        yv_in_len = (size_t)got;
        return true;
    }
#endif
}

static int yv_utf8_status(const yv_string *s, int64_t *code) {
    int64_t bad = yv_utf8_check(s->ptr ? s->ptr : "", (size_t)s->len);
    if (bad < 0) return 0;
    *code = bad;
    return 1;
}

/* `io.readAll()`: the rest of standard input. */
static int yv_io_read_all(yv_string *out, int64_t *code, uint32_t line, uint32_t col) {
    int64_t err = 0;
    for (;;) {
        if (yv_in_pos < yv_in_len) {
            yv_str t = {yv_in_buf + yv_in_pos, yv_in_len - yv_in_pos};
            yv_string_push(out, t, line, col);
            yv_in_pos = yv_in_len;
        }
        if (!yv_in_fill(&err)) break;
    }
    if (err != 0) {
        *code = err;
        return 2;
    }
    return yv_utf8_status(out, code);
}

/* `io.readLine()`: the next line without its `\n` (or `\r\n`); `*has` is false at the end. */
static int yv_io_read_line(yv_string *out, bool *has, int64_t *code, uint32_t line, uint32_t col) {
    int64_t err = 0;
    *has = false;
    for (;;) {
        if (yv_in_pos == yv_in_len && !yv_in_fill(&err)) break;
        char *start = yv_in_buf + yv_in_pos;
        char *nl = (char *)memchr(start, '\n', yv_in_len - yv_in_pos);
        size_t n = nl ? (size_t)(nl - start) : yv_in_len - yv_in_pos;
        yv_str t = {start, n};
        yv_string_push(out, t, line, col);
        *has = true;
        yv_in_pos += n;
        if (nl) {
            yv_in_pos++;
            if (out->len > 0 && out->ptr[out->len - 1] == '\r') out->len--;
            break;
        }
    }
    if (err != 0) {
        *code = err;
        return 2;
    }
    return yv_utf8_status(out, code);
}

/* A NUL-terminated copy of a path (not counted as a program allocation); NULL if it has a NUL. */
static char *yv_path_copy(yv_str path, uint32_t line, uint32_t col) {
    if (memchr(path.ptr, 0, path.len) != NULL) return NULL;
    char *p = (char *)malloc(path.len + 1);
    if (p == NULL) yv_panic("P-OOM", "out of memory", line, col);
    memcpy(p, path.ptr, path.len);
    p[path.len] = 0;
    return p;
}

#ifdef _WIN32
/* The UTF-16 form of a NUL-terminated UTF-8 path, or NULL. */
static wchar_t *yv_wide(const char *s) {
    int n = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, s, -1, NULL, 0);
    if (n <= 0) return NULL;
    wchar_t *w = (wchar_t *)malloc((size_t)n * sizeof(wchar_t));
    if (w == NULL) return NULL;
    MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, s, -1, w, n);
    return w;
}
#endif

/* `fs.readText(path)`. */
static int yv_fs_read_text(yv_str path, yv_string *out, int64_t *code, uint32_t line, uint32_t col) {
    char *p = yv_path_copy(path, line, col);
    if (p == NULL) return 3;
    char buf[65536];
#ifdef _WIN32
    wchar_t *w = yv_wide(p);
    free(p);
    if (w == NULL) return 3;
    HANDLE h = CreateFileW(w, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
    free(w);
    if (h == INVALID_HANDLE_VALUE) {
        *code = (int64_t)GetLastError();
        return 2;
    }
    for (;;) {
        DWORD got = 0;
        if (!ReadFile(h, buf, (DWORD)sizeof buf, &got, NULL)) {
            *code = (int64_t)GetLastError();
            CloseHandle(h);
            return 2;
        }
        if (got == 0) break;
        yv_str t = {buf, got};
        yv_string_push(out, t, line, col);
    }
    CloseHandle(h);
#else
    int fd;
    do fd = open(p, O_RDONLY);
    while (fd < 0 && errno == EINTR);
    free(p);
    if (fd < 0) {
        *code = errno;
        return 2;
    }
    for (;;) {
        ssize_t got = read(fd, buf, sizeof buf);
        if (got < 0 && errno == EINTR) continue;
        if (got < 0) {
            *code = errno;
            close(fd);
            return 2;
        }
        if (got == 0) break;
        yv_str t = {buf, (size_t)got};
        yv_string_push(out, t, line, col);
    }
    close(fd);
#endif
    return yv_utf8_status(out, code);
}

/* `fs.writeText(path, text)`: creates or truncates the file. */
static int yv_fs_write_text(yv_str path, yv_str text, int64_t *code, uint32_t line, uint32_t col) {
    char *p = yv_path_copy(path, line, col);
    if (p == NULL) return 3;
#ifdef _WIN32
    wchar_t *w = yv_wide(p);
    free(p);
    if (w == NULL) return 3;
    HANDLE h = CreateFileW(w, GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
    free(w);
    if (h == INVALID_HANDLE_VALUE) {
        *code = (int64_t)GetLastError();
        return 2;
    }
    size_t done = 0;
    while (done < text.len) {
        DWORD chunk = text.len - done > 0x40000000 ? 0x40000000 : (DWORD)(text.len - done), put = 0;
        if (!WriteFile(h, text.ptr + done, chunk, &put, NULL)) {
            *code = (int64_t)GetLastError();
            CloseHandle(h);
            return 2;
        }
        done += put;
    }
    if (!CloseHandle(h)) {
        *code = (int64_t)GetLastError();
        return 2;
    }
#else
    int fd;
    do fd = open(p, O_WRONLY | O_CREAT | O_TRUNC, 0666);
    while (fd < 0 && errno == EINTR);
    free(p);
    if (fd < 0) {
        *code = errno;
        return 2;
    }
    size_t done = 0;
    while (done < text.len) {
        ssize_t put = write(fd, text.ptr + done, text.len - done);
        if (put < 0 && errno == EINTR) continue;
        if (put < 0) {
            *code = errno;
            close(fd);
            return 2;
        }
        done += (size_t)put;
    }
    if (close(fd) != 0) {
        *code = errno;
        return 2;
    }
#endif
    (void)line;
    (void)col;
    return 0;
}

/* `os.args()`: converted and checked once at startup (S5, M5); the strings live until exit. */
static yv_vec yv_args = {0};

YV_NORETURN static void yv_startup_utf8(int64_t index) {
    static const char msg[] = "yavo: YAVO-STARTUP-UTF8: command-line argument ";
    (void)yv_os_write(2, msg, sizeof msg - 1);
    yv_os_write_u32(2, (uint32_t)index);
    static const char tail[] = " is not valid UTF-8\n";
    (void)yv_os_write(2, tail, sizeof tail - 1);
    yv_exit_process(2);
}

static void yv_args_push(const char *s, size_t n) {
    if (yv_args.len == yv_args.cap) {
        int64_t cap = yv_args.cap ? yv_args.cap * 2 : 8;
        void *p = realloc(yv_args.ptr, (size_t)cap * sizeof(yv_str));
        if (p == NULL) yv_panic("P-OOM", "out of memory", 0, 0);
        yv_args.ptr = p;
        yv_args.cap = cap;
    }
    yv_str v = {s, n};
    ((yv_str *)yv_args.ptr)[yv_args.len++] = v;
}

#ifdef _WIN32
#include <shellapi.h>
static void yv_rt_args(int argc, char **argv) {
    (void)argc;
    (void)argv;
    int n = 0;
    LPWSTR *w = CommandLineToArgvW(GetCommandLineW(), &n);
    if (w == NULL) return;
    for (int i = 0; i < n; i++) {
        int len = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, w[i], -1, NULL, 0, NULL, NULL);
        if (len <= 0) yv_startup_utf8(i);
        char *s = (char *)malloc((size_t)len);
        if (s == NULL) yv_panic("P-OOM", "out of memory", 0, 0);
        WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, w[i], -1, s, len, NULL, NULL);
        yv_args_push(s, (size_t)len - 1);
    }
    LocalFree(w);
}
#else
static void yv_rt_args(int argc, char **argv) {
    for (int i = 0; i < argc; i++) {
        size_t n = strlen(argv[i]);
        if (yv_utf8_check(argv[i], n) >= 0) yv_startup_utf8(i);
        yv_args_push(argv[i], n);
    }
}
#endif


/* ---- Concurrency (semantic table P1-P8, stdlib contract §6). ----
 * OS threads only. Locks and condition variables are pthread on Unix and SRWLOCK /
 * CONDITION_VARIABLE on Windows. `Rc`, `Arc` and `Mutex` values point at their content, which
 * follows a `yv_hdr` in the same heap block; a `Guard` is the same pointer while the lock is
 * held. Output is serialized once a second thread has started. */
#ifdef _WIN32
typedef SRWLOCK yv_lockt;
typedef CONDITION_VARIABLE yv_condt;
typedef HANDLE yv_thread;
#define YV_LOCK_INIT SRWLOCK_INIT
static void yv_lock_init(yv_lockt *l) { InitializeSRWLock(l); }
static void yv_lock_acq(yv_lockt *l) { AcquireSRWLockExclusive(l); }
static bool yv_lock_try(yv_lockt *l) { return TryAcquireSRWLockExclusive(l) != 0; }
static void yv_lock_rel(yv_lockt *l) { ReleaseSRWLockExclusive(l); }
static void yv_lock_destroy(yv_lockt *l) { (void)l; }
static void yv_cond_init(yv_condt *c) { InitializeConditionVariable(c); }
static void yv_cond_wait(yv_condt *c, yv_lockt *l) { SleepConditionVariableSRW(c, l, INFINITE, 0); }
static void yv_cond_wake(yv_condt *c) { WakeAllConditionVariable(c); }
static void yv_cond_destroy(yv_condt *c) { (void)c; }
#else
#include <pthread.h>
typedef pthread_mutex_t yv_lockt;
typedef pthread_cond_t yv_condt;
typedef pthread_t yv_thread;
#define YV_LOCK_INIT PTHREAD_MUTEX_INITIALIZER
static void yv_lock_init(yv_lockt *l) {
    if (pthread_mutex_init(l, NULL) != 0) yv_panic("P-SYSTEM", "cannot create a lock", 0, 0);
}
static void yv_lock_acq(yv_lockt *l) {
    if (pthread_mutex_lock(l) != 0) yv_panic("P-SYSTEM", "cannot acquire a lock", 0, 0);
}
static bool yv_lock_try(yv_lockt *l) { return pthread_mutex_trylock(l) == 0; }
static void yv_lock_rel(yv_lockt *l) { (void)pthread_mutex_unlock(l); }
static void yv_lock_destroy(yv_lockt *l) { (void)pthread_mutex_destroy(l); }
static void yv_cond_init(yv_condt *c) {
    if (pthread_cond_init(c, NULL) != 0) yv_panic("P-SYSTEM", "cannot create a condition variable", 0, 0);
}
static void yv_cond_wait(yv_condt *c, yv_lockt *l) { (void)pthread_cond_wait(c, l); }
static void yv_cond_wake(yv_condt *c) { (void)pthread_cond_broadcast(c); }
static void yv_cond_destroy(yv_condt *c) { (void)pthread_cond_destroy(c); }
#endif

/* Small per-thread ids; 0 means "nobody". */
static uint64_t yv_next_tid = 1;
static _Thread_local uint64_t yv_tid = 0;
static uint64_t yv_thread_id(void) {
    if (yv_tid == 0) yv_tid = __atomic_fetch_add(&yv_next_tid, 1, __ATOMIC_RELAXED);
    return yv_tid;
}

/* Output lock: print statements take it once a second thread may be running. */
static bool yv_threaded = false;
static yv_lockt yv_out_lk = YV_LOCK_INIT;
static void yv_out_lock(void) {
    if (__atomic_load_n(&yv_threaded, __ATOMIC_ACQUIRE)) yv_lock_acq(&yv_out_lk);
}
static void yv_out_unlock(void) {
    if (__atomic_load_n(&yv_threaded, __ATOMIC_ACQUIRE)) yv_lock_rel(&yv_out_lk);
}

/* ---- Header blocks for Rc, Arc and Mutex. ---- */
typedef struct {
    yv_lockt lk;
    uint64_t owner;  /* Mutex: the thread holding the lock, 0 if free */
    int64_t count;   /* Rc/Arc: number of handles */
} yv_hdr;
#define YV_HDR_SIZE ((sizeof(yv_hdr) + 15) & ~(size_t)15)

static void *yv_hdr_alloc(size_t size, bool with_lock, uint32_t line, uint32_t col) {
    char *base = (char *)yv_box_alloc(YV_HDR_SIZE + size, line, col);
    yv_hdr *h = (yv_hdr *)base;
    if (with_lock) yv_lock_init(&h->lk);
    h->owner = 0;
    h->count = 1;
    return base + YV_HDR_SIZE;
}
static yv_hdr *yv_hdr_of(const void *p) { return (yv_hdr *)((char *)p - YV_HDR_SIZE); }
static void yv_hdr_free(void *p, bool with_lock) {
    yv_hdr *h = yv_hdr_of(p);
    if (with_lock) yv_lock_destroy(&h->lk);
    yv_box_free(h);
}

/* Rc: plain counts (never shared between threads). Arc: atomic counts. Release returns
 * true when the last handle is gone and the content must be dropped. */
static void *yv_rc_retain(void *p) {
    yv_hdr_of(p)->count++;
    return p;
}
static bool yv_rc_release(void *p) { return --yv_hdr_of(p)->count == 0; }
static void *yv_arc_retain(void *p) {
    __atomic_add_fetch(&yv_hdr_of(p)->count, 1, __ATOMIC_RELAXED);
    return p;
}
static bool yv_arc_release(void *p) { return __atomic_sub_fetch(&yv_hdr_of(p)->count, 1, __ATOMIC_ACQ_REL) == 0; }

/* Mutex: `lock` blocks; locking again on the holding thread panics (P8). `tryLock` fails on
 * any contention, the holding thread included. */
static void *yv_mutex_lock(void *m, uint32_t line, uint32_t col) {
    yv_hdr *h = yv_hdr_of(m);
    uint64_t me = yv_thread_id();
    if (__atomic_load_n(&h->owner, __ATOMIC_RELAXED) == me) {
        yv_panic("P-LOCK-REENTRANT", "this thread already holds the lock", line, col);
    }
    yv_lock_acq(&h->lk);
    __atomic_store_n(&h->owner, me, __ATOMIC_RELAXED);
    return m;
}
static void *yv_mutex_try_lock(void *m) {
    yv_hdr *h = yv_hdr_of(m);
    if (__atomic_load_n(&h->owner, __ATOMIC_RELAXED) == yv_thread_id()) return NULL;
    if (!yv_lock_try(&h->lk)) return NULL;
    __atomic_store_n(&h->owner, yv_thread_id(), __ATOMIC_RELAXED);
    return m;
}
static void yv_mutex_unlock(void *m) {
    yv_hdr *h = yv_hdr_of(m);
    __atomic_store_n(&h->owner, 0, __ATOMIC_RELAXED);
    yv_lock_rel(&h->lk);
}

/* ---- Channel[T]: a bounded queue of `esz`-byte values. ---- */
typedef struct yv_chan {
    yv_lockt lk;
    yv_condt nonempty, nonfull;
    int64_t refs;
    bool closed;
    int64_t cap, head, len;
    size_t esz;
    char *buf;
    void (*drop)(void *);
} yv_chan;

static yv_chan *yv_chan_new(int64_t cap, size_t esz, void (*drop)(void *), uint32_t line, uint32_t col) {
    if (cap <= 0) yv_panic("P-CHANNEL-CAP", "channel capacity must be positive", line, col);
    size_t bytes;
    if (__builtin_mul_overflow((size_t)cap, esz ? esz : 1, &bytes) || bytes > (size_t)PTRDIFF_MAX) yv_capacity_overflow(line, col);
    yv_chan *c = (yv_chan *)yv_box_alloc(sizeof(yv_chan), line, col);
    c->buf = (char *)yv_box_alloc(bytes, line, col);
    yv_lock_init(&c->lk);
    yv_cond_init(&c->nonempty);
    yv_cond_init(&c->nonfull);
    c->refs = 1;
    c->closed = false;
    c->cap = cap;
    c->head = c->len = 0;
    c->esz = esz;
    c->drop = drop;
    return c;
}
/* Moves `*v` into the queue (true), or leaves it with the caller when closed (false). */
static bool yv_chan_send(yv_chan *c, const void *v) {
    yv_lock_acq(&c->lk);
    while (c->len == c->cap && !c->closed) yv_cond_wait(&c->nonfull, &c->lk);
    if (c->closed) {
        yv_lock_rel(&c->lk);
        return false;
    }
    memcpy(c->buf + (size_t)((c->head + c->len) % c->cap) * c->esz, v, c->esz);
    c->len++;
    yv_cond_wake(&c->nonempty);
    yv_lock_rel(&c->lk);
    return true;
}
/* Moves the oldest value into `*out` (true); false once closed and empty. */
static bool yv_chan_recv(yv_chan *c, void *out) {
    yv_lock_acq(&c->lk);
    while (c->len == 0 && !c->closed) yv_cond_wait(&c->nonempty, &c->lk);
    if (c->len == 0) {
        yv_lock_rel(&c->lk);
        return false;
    }
    memcpy(out, c->buf + (size_t)c->head * c->esz, c->esz);
    c->head = (c->head + 1) % c->cap;
    c->len--;
    yv_cond_wake(&c->nonfull);
    yv_lock_rel(&c->lk);
    return true;
}
static void yv_chan_close(yv_chan *c) {
    yv_lock_acq(&c->lk);
    c->closed = true;
    yv_cond_wake(&c->nonempty);
    yv_cond_wake(&c->nonfull);
    yv_lock_rel(&c->lk);
}
static yv_chan *yv_chan_clone(yv_chan *c) {
    __atomic_add_fetch(&c->refs, 1, __ATOMIC_RELAXED);
    return c;
}
static void yv_chan_drop(yv_chan **p) {
    yv_chan *c = *p;
    *p = NULL;
    if (c == NULL || __atomic_sub_fetch(&c->refs, 1, __ATOMIC_ACQ_REL) != 0) return;
    if (c->drop) {
        for (int64_t i = 0; i < c->len; i++) c->drop(c->buf + (size_t)((c->head + i) % c->cap) * c->esz);
    }
    yv_box_free(c->buf);
    yv_lock_destroy(&c->lk);
    yv_cond_destroy(&c->nonempty);
    yv_cond_destroy(&c->nonfull);
    yv_box_free(c);
}

/* ---- Threads: `scope` and `spawn`. ----
 * Every spawned thread has a join record owned by its scope; dropping the scope (normal end,
 * `return`, `break`, `try`) waits for the threads that were not joined, drops results nobody
 * took and frees the records (P3). */
typedef struct yv_join_base {
    yv_thread th;
    bool joined, taken;
    void (*drop_ret)(void *);
    void *ret;
    void (*run)(void *);
    void *args;
} yv_join_base;

typedef struct {
    yv_join_base **items;
    int64_t len, cap;
} yv_scope;

#ifdef _WIN32
static DWORD WINAPI yv_thread_main(LPVOID p) {
    yv_join_base *j = (yv_join_base *)p;
    j->run(j->args);
    return 0;
}
#else
static void *yv_thread_main(void *p) {
    yv_join_base *j = (yv_join_base *)p;
    j->run(j->args);
    return NULL;
}
#endif

/* Starts `run(args)` on a new thread for the record `j` and adds it to scope `s`. */
static void yv_spawn(yv_scope *s, yv_join_base *j, void (*run)(void *), void *args, uint32_t line, uint32_t col) {
    if (s->len == s->cap) {
        int64_t ncap = s->cap ? s->cap * 2 : 4;
        yv_join_base **items = (yv_join_base **)realloc(s->items, (size_t)ncap * sizeof *items);
        if (items == NULL) yv_panic("P-OOM", "out of memory", line, col);
        if (s->items == NULL) YV_COUNT_ALLOC();
        s->items = items;
        s->cap = ncap;
    }
    s->items[s->len++] = j;
    j->run = run;
    j->args = args;
    __atomic_store_n(&yv_threaded, true, __ATOMIC_RELEASE);
#ifdef _WIN32
    j->th = CreateThread(NULL, 0, yv_thread_main, j, 0, NULL);
    if (j->th == NULL) yv_panic("P-THREAD-SPAWN", "cannot start a thread", line, col);
#else
    if (pthread_create(&j->th, NULL, yv_thread_main, j) != 0) yv_panic("P-THREAD-SPAWN", "cannot start a thread", line, col);
#endif
}

static void yv_join_wait(yv_join_base *j) {
    if (j->joined) return;
#ifdef _WIN32
    WaitForSingleObject(j->th, INFINITE);
    CloseHandle(j->th);
#else
    (void)pthread_join(j->th, NULL);
#endif
    j->joined = true;
}

static void yv_drop_Scope(yv_scope *s) {
    for (int64_t i = 0; i < s->len; i++) yv_join_wait(s->items[i]);
    for (int64_t i = 0; i < s->len; i++) {
        yv_join_base *j = s->items[i];
        if (!j->taken && j->drop_ret) j->drop_ret(j->ret);
        yv_box_free(j);
    }
    if (s->items) yv_box_free(s->items);
    s->items = NULL;
    s->len = s->cap = 0;
}

#endif
