#include "common.h"
typedef struct { void *ptr; int64_t len, cap, head; } deque;
static int64_t index_of(const deque *q, int64_t logical) {
    int64_t tail = q->cap - q->head;
    return logical < tail ? q->head + logical : logical - tail;
}
static void push_back(deque *q, int64_t value) {
    int64_t need = q->len + 1;
    if (need > q->cap) {
        int64_t cap = 4;
        while (cap < need) cap *= 2;
        int64_t *fresh = realloc(NULL, (size_t)cap * sizeof(int64_t));
        require(fresh != NULL);
        if (q->len) {
            int64_t first = q->cap - q->head;
            if (first > q->len) first = q->len;
            memcpy(fresh, (int64_t *)q->ptr + q->head, (size_t)first * sizeof(int64_t));
            if (first < q->len) memcpy(fresh + first, q->ptr, (size_t)(q->len - first) * sizeof(int64_t));
        }
        if (q->ptr) free(q->ptr);
        q->ptr = fresh; q->cap = cap; q->head = 0;
    }
    ((int64_t *)q->ptr)[index_of(q, q->len)] = value;
    q->len = need;
}
static int pop_front(deque *q, int64_t *out) {
    if (!q->len) return 0;
    int64_t *slot = &((int64_t *)q->ptr)[q->head];
    *out = *slot; *slot = 0;
    q->len--;
    if (!q->len) q->head = 0;
    else q->head = q->head == q->cap - 1 ? 0 : q->head + 1;
    return 1;
}
int main(int argc, char **argv) {
    int64_t n, work, seed;
    inputs(argc, argv, &n, &work, &seed, 2000000, 20000000);
    deque q = {0};
    for (int64_t i = 0; i < n; i++) push_back(&q, (i * 17 + seed) % 65521);
    int64_t checksum = 0;
    for (int64_t i = 0; i < work; i++) {
        int64_t value = -1;
        pop_front(&q, &value);
        checksum += value;
        push_back(&q, (value + 1) % 65521);
    }
    int64_t value;
    while (pop_front(&q, &value)) checksum += value;
    free(q.ptr);
    printf("%" PRId64 "\n", checksum);
    return 0;
}
