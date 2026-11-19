/* Yavo P0 runtime. Included by every generated C file; requires C17 and a compiler
 * with the GCC/Clang overflow builtins (the pinned `zig cc` provides both). */
#ifndef YAVO_RT_H
#define YAVO_RT_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#else
#include <errno.h>
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
#define YV_COUNT_ALLOC() (yv_live_blocks++)
#define YV_COUNT_FREE()                                                                   \
    do {                                                                                  \
        if (--yv_live_blocks < 0) {                                                       \
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
YV_NORETURN static void yv_panic(const char *category, const char *message, uint32_t line, uint32_t col) {
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
    yv_write(2, message, strlen(message));
    yv_write(2, "\n", 1);
    yv_exit_process(YV_PANIC_EXIT_CODE);
    YV_UNREACHABLE();
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

#endif
