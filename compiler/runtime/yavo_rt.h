/* Yavo P0 runtime. Included by every generated C file; requires C17 and a compiler
 * with the GCC/Clang overflow builtins (the pinned `zig cc` provides both). */
#ifndef YAVO_RT_H
#define YAVO_RT_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
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

static int yv_rt_exit(void) {
    yv_flush();
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

#endif
