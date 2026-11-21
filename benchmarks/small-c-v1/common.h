#ifndef YAVO_SMALL_C_COMMON_H
#define YAVO_SMALL_C_COMMON_H
#include <errno.h>
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void require(int condition) {
    if (!condition) { fputs("invalid benchmark input or allocation failure\n", stderr); exit(2); }
}
static int64_t number(const char *s) {
    char *end;
    errno = 0;
    int64_t value = strtoll(s, &end, 10);
    require(!errno && end != s && *end == 0);
    return value;
}
static void inputs(int argc, char **argv, int64_t *n, int64_t *work, int64_t *seed,
                   int64_t max_n, int64_t max_work) {
    require(argc == 4);
    *n = number(argv[1]); *work = number(argv[2]); *seed = number(argv[3]);
    require(*n > 0 && *n <= max_n && *work > 0 && *work <= max_work
            && *seed > 0 && *seed <= 100000);
}
typedef struct { void *ptr; int64_t len, cap; } vec;
static vec make_vec(int64_t n, int64_t seed) {
    int64_t cap = 4;
    while (cap < n) cap *= 2;
    vec v = {realloc(NULL, (size_t)cap * sizeof(int64_t)), 0, cap};
    require(v.ptr != NULL);
    for (int64_t i = 0; i < n; i++) {
        ((int64_t *)v.ptr)[v.len++] = (i * 17 + seed) % 65521;
    }
    return v;
}
static int64_t sum_vec(const vec *v) {
    int64_t sum = 0;
    for (int64_t i = 0; i < v->len; i++) sum += ((int64_t *)v->ptr)[i];
    return sum;
}
#endif
