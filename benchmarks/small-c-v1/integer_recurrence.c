#include "common.h"
int main(int argc, char **argv) {
    int64_t n, work, seed;
    inputs(argc, argv, &n, &work, &seed, 20000000, 100);
    int64_t x = seed, checksum = 0;
    for (int64_t round = 0; round < work; round++) {
        for (int64_t i = 0; i < n; i++) {
            x = (x * 1664525 + 1013904223) % 2147483647;
            checksum = (checksum + x) % 1000000007;
        }
    }
    printf("%" PRId64 "\n", checksum);
    return 0;
}
