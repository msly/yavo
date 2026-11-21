#include "common.h"
int main(int argc, char **argv) {
    int64_t n, work, seed;
    inputs(argc, argv, &n, &work, &seed, 2000000, 1000);
    vec values = make_vec(n, seed);
    int64_t *p = values.ptr;
    for (int64_t round = 0; round < work; round++) {
        for (int64_t i = 0; i < n; i++) {
            p[i] = (p[i] * 3 + round) % 65521;
        }
    }
    int64_t checksum = sum_vec(&values);
    free(values.ptr);
    printf("%" PRId64 "\n", checksum);
    return 0;
}
