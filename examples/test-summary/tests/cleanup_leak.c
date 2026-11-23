/* Deliberately leak a real runtime String; the injected exit check must detect it. */
#include "yavo_rt.h"
int main(void) {
    yv_rt_init("test-summary-cleanup-leak");
    yv_string text = yv_string_from((yv_str){"probe", 5}, 1, 1);
    (void)text;
    yv_os_exit(2);
}
