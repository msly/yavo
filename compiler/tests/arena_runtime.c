/* Test-only access to counters exercises exhaustion without enormous allocations. */
#include "yavo_rt.h"
typedef struct { yv_arena_slot header; int64_t val; } int_entry;
enum { THREADS = 8, EACH = 4096 };
static uint64_t identities[THREADS * EACH];
static void create_arenas(void *arg) {
    size_t row = (size_t)(uintptr_t)arg;
    for (size_t i = 0; i < EACH; i++) {
        yv_arena a = yv_arena_new(1, 1);
        identities[row * EACH + i] = a.identity;
        yv_arena_free(&a);
    }
}
static int compare_identity(const void *a, const void *b) {
    uint64_t x = *(const uint64_t *)a, y = *(const uint64_t *)b;
    return x < y ? -1 : x > y;
}
int main(int argc, char **argv) {
    yv_rt_init("arena-runtime.yv");
    if (argc != 2) return 2;
    if (strcmp(argv[1], "concurrent") == 0) {
        yv_scope scope = {0};
        for (uintptr_t i = 0; i < THREADS; i++) {
            yv_join_base *j = yv_box_alloc(sizeof(*j), 1, 1);
            memset(j, 0, sizeof(*j));
            yv_spawn(&scope, j, create_arenas, (void *)i, 1, 1);
        }
        yv_drop_Scope(&scope);
        qsort(identities, THREADS * EACH, sizeof(uint64_t), compare_identity);
        const uint64_t *p = identities;
        for (size_t i = 0; i < THREADS * EACH; i++) if (p[i] != i + 1) return 20;
        return yv_rt_exit();
    }
    if (strcmp(argv[1], "identity") == 0) {
        __atomic_store_n(&yv_arena_identity_counter, UINT64_MAX, __ATOMIC_RELAXED);
        (void)yv_arena_new(31, 5);
        return 21;
    }
    yv_arena a = yv_arena_new(1, 1);
    if (__atomic_load_n(&yv_live_blocks, __ATOMIC_RELAXED) != 0) return 22;
    if (strcmp(argv[1], "capacity") == 0) {
        a.slots.len = INT64_MAX;
        (void)yv_arena_add_slot(&a, sizeof(int_entry), 31, 5);
        return 23;
    }
    yv_id id = yv_arena_add_slot(&a, sizeof(int_entry), 1, 1);
    int_entry *s = (int_entry *)yv_arena_entry(&a, sizeof(int_entry), id.slot);
    s->val = 7;
    if (strcmp(argv[1], "generation") == 0) {
        s->header.generation = UINT64_MAX;
        id.generation = UINT64_MAX;
        (void)yv_arena_remove(&a, id, sizeof(int_entry), 1, 1);
        (void)yv_arena_add_slot(&a, sizeof(int_entry), 31, 5);
        return 24;
    }
    if (strcmp(argv[1], "bounds") == 0) {
        yv_id out = id;
        out.slot = UINT64_MAX;
        if (yv_arena_get(&a, out, sizeof(int_entry), 1, 1) != NULL) return 25;
        if (yv_arena_remove(&a, out, sizeof(int_entry), 1, 1) != NULL) return 26;
        (void)yv_arena_remove(&a, id, sizeof(int_entry), 1, 1);
        yv_id replacement = yv_arena_add_slot(&a, sizeof(int_entry), 1, 1);
        if (replacement.slot != id.slot || replacement.generation <= id.generation) return 27;
        if (yv_arena_get(&a, id, sizeof(int_entry), 1, 1) != NULL) return 28;
        yv_arena_free(&a);
        return yv_rt_exit();
    }
    return 2;
}
