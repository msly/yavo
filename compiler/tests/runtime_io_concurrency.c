#define _DEFAULT_SOURCE 1
#define _XOPEN_SOURCE 700
#include <stdlib.h>
static _Thread_local int fail_alloc;
static void *controlled_malloc(size_t n) { return fail_alloc ? NULL : malloc(n); }
static void *controlled_realloc(void *p, size_t n) { return fail_alloc ? NULL : realloc(p, n); }
#define malloc controlled_malloc
#define realloc controlled_realloc
#include "yavo_rt.h"
#undef malloc
#undef realloc
#ifndef _WIN32
#include <time.h>
#endif

/* Deterministic ordering probes for runtime I/O, including fatal paths. */
static int ready, pending, finished;

static void pause_ms(unsigned ms) {
#ifdef _WIN32
    Sleep(ms);
#else
    struct timespec duration = {(time_t)(ms / 1000), (long)(ms % 1000) * 1000000};
    while (nanosleep(&duration, &duration) != 0 && errno == EINTR) {}
#endif
}

static void wait_for(int *flag) {
    while (!__atomic_load_n(flag, __ATOMIC_ACQUIRE)) pause_ms(1);
}

static void launch(yv_scope *scope, void (*body)(void *)) {
    yv_join_base *j = (yv_join_base *)yv_box_alloc(sizeof *j, 1, 1);
    memset(j, 0, sizeof *j);
    yv_spawn(scope, j, body, NULL, 1, 1);
}

static void locked_writer(void *unused) {
    (void)unused;
    yv_out_lock();
    yv_output_at(11, 4);
    yv_write(1, "before", 6);
    __atomic_store_n(&ready, 1, __ATOMIC_RELEASE);
    wait_for(&pending);
    /* Give the competing fatal/read path a chance to attempt its flush while the
     * statement owns the output lock. The correct implementation waits for us. */
    pause_ms(100);
    yv_write(1, "after", 5);
    __atomic_store_n(&finished, 1, __ATOMIC_RELEASE);
    yv_out_unlock();
}

static void other_location(void *unused) {
    (void)unused;
    yv_output_at(22, 8);
}

int main(int argc, char **argv) {
    yv_rt_init("io-lock.yv");
    if (argc != 2) return 2;
    yv_scope scope = {0};
    if (strcmp(argv[1], "location") == 0) {
        yv_output_at(11, 4);
        launch(&scope, other_location);
        yv_drop_Scope(&scope);
        char bytes[9000];
        memset(bytes, 'x', sizeof bytes);
#ifdef _WIN32
        CloseHandle(GetStdHandle(STD_OUTPUT_HANDLE));
#else
        close(1);
#endif
        yv_out_lock();
        yv_write(1, bytes, sizeof bytes);
        yv_out_unlock();
        return yv_rt_exit();
    }
    if (strcmp(argv[1], "nested-panic") == 0) {
        yv_out_lock();
        yv_write(1, "nested", 6);
        yv_panic("P-PANIC", "nested panic", 31, 7);
    }
    if (strcmp(argv[1], "nested-exit") == 0) {
        yv_out_lock();
        yv_write(1, "nested", 6);
        yv_os_exit(23);
    }
    if (strcmp(argv[1], "nested-oom") == 0) {
        yv_out_lock();
        yv_write(1, "nested", 6);
        fail_alloc = 1;
        (void)yv_box_alloc(1, 32, 8);
        return 79;
    }
    if (strcmp(argv[1], "input-oom") == 0) {
        yv_string line = {0};
        int64_t code = 0;
        bool has = false;
        fail_alloc = 1;
        (void)yv_io_read_line(&line, &has, &code, 33, 9);
        return 79;
    }
    launch(&scope, locked_writer);
    wait_for(&ready);
    __atomic_store_n(&pending, 1, __ATOMIC_RELEASE);
    if (strcmp(argv[1], "panic") == 0) yv_panic("P-PANIC", "other thread", 41, 9);
    if (strcmp(argv[1], "exit") == 0) yv_os_exit(23);
    if (strcmp(argv[1], "input") == 0) {
        yv_string line = {0};
        int64_t code = 0;
        bool has = false;
        int status = yv_io_read_line(&line, &has, &code, 51, 3);
        if (!__atomic_load_n(&finished, __ATOMIC_ACQUIRE)) return 77;
        if (status != 0 || !has || line.len != 5 || memcmp(line.ptr, "input", 5) != 0) return 78;
        yv_string_drop(&line);
        yv_drop_Scope(&scope);
        return yv_rt_exit();
    }
    return 2;
}
