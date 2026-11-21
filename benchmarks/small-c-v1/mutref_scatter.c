#include "common.h"
static int64_t *cell(vec *values, int64_t i) { return &((int64_t *)values->ptr)[i]; }
int main(int argc, char **argv) {
    int64_t n, work, seed;
    inputs(argc, argv, &n, &work, &seed, 2000000, 20000000);
    vec values = make_vec(n, seed);
    int64_t x = seed;
    for (int64_t i = 0; i < work; i++) {
        x = (x * 48271) % 2147483647;
        int64_t *p = cell(&values, x % n);
        *p = (*p + 1) % 65521;
    }
    int64_t checksum = sum_vec(&values);
    free(values.ptr);
    printf("%" PRId64 "\n", checksum);
    return 0;
}
