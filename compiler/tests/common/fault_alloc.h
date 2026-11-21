#if !defined(_WIN32)
#define _DEFAULT_SOURCE 1
#define _XOPEN_SOURCE 700
#endif
/* Test-only allocation failure injection. Never included by production code.
 * Include before the generated translation unit: allocations in yv_rt_args
 * remain real, then the configured allocation fails after entry to user main.
 * The allocator wrappers exist only inside this test translation unit. */
#ifndef YV_TEST_FAIL_AT
#error YV_TEST_FAIL_AT must be a positive allocation index
#endif
#if YV_TEST_FAIL_AT < 1
#error YV_TEST_FAIL_AT must be positive
#endif
#include <stdlib.h>
#include <stdint.h>

static uint64_t yv_test_allocations = 0;
static int yv_test_fault_armed = 0;
static int yv_test_allocation_fails(void) {
    return __atomic_load_n(&yv_test_fault_armed, __ATOMIC_ACQUIRE) &&
        __atomic_add_fetch(&yv_test_allocations, 1, __ATOMIC_RELAXED) == YV_TEST_FAIL_AT;
}
static void *yv_test_malloc(size_t n) {
    return yv_test_allocation_fails() ? NULL : malloc(n);
}
static void *yv_test_calloc(size_t n, size_t size) {
    return yv_test_allocation_fails() ? NULL : calloc(n, size);
}
static void *yv_test_realloc(void *p, size_t n) {
    return yv_test_allocation_fails() ? NULL : realloc(p, n);
}
#define malloc yv_test_malloc
#define calloc yv_test_calloc
#define realloc yv_test_realloc
#include "yavo_rt.h"
#undef malloc
#undef calloc
#undef realloc

static void yv_test_rt_args(int argc, char **argv) {
    yv_rt_args(argc, argv);
    __atomic_store_n(&yv_test_fault_armed, 1, __ATOMIC_RELEASE);
}
#define yv_rt_args yv_test_rt_args
