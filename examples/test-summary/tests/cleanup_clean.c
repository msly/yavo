/* A real tracked allocation is freed before an explicit error-code exit. */
#include "yavo_rt.h"
int main(void) {
    yv_rt_init("test-summary-cleanup-clean");
    yv_string text = yv_string_from((yv_str){"probe", 5}, 1, 1);
    yv_string_drop(&text);
    yv_os_exit(2);
}
