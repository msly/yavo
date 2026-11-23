/* Test-only forced include. The original generated C and runtime stay unchanged.
 * The runtime include guard keeps the original C's own include harmless. */
#include "yavo_rt.h"

YV_NORETURN static void yavo_test_summary_checked_exit(int64_t code) {
    int allocation_status = yv_rt_exit();
    if (allocation_status != 0) yv_os_exit(allocation_status);
    yv_os_exit(code);
}

/* Defined after the wrapper so its calls still reach the original runtime. */
#define yv_os_exit yavo_test_summary_checked_exit
