/* Independent public-system-runtime regression. The helper branch intentionally
 * does not include yavo_rt.h and is built without sanitizer flags. */
#ifdef YV_SYSTEM_HELPER
#include <stdio.h>
#include <string.h>
#include <signal.h>
int main(int argc, char **argv) {
    if (argc < 2) return 90;
    if (strcmp(argv[1], "exit37") == 0) return 37;
    if (strcmp(argv[1], "exit127") == 0) return 127;
    if (strcmp(argv[1], "signal") == 0) { raise(SIGTERM); return 99; }
    if (strcmp(argv[1], "streams") == 0) {
        fputs("probe-stdout\n", stdout); fflush(stdout);
        fputs("probe-stderr\n", stderr); return 0;
    }
    if (strcmp(argv[1], "argv") == 0) {
        const char *wanted[] = {"", "a b", "x\"y", "tail\\", "汉🙂", ";$(touch no-shell)"};
        if (argc != 8) return 91;
        for (int i = 0; i < 6; i++) if (strcmp(argv[i + 2], wanted[i]) != 0) return 92;
        fputs("argv-ok\n", stdout); return 0;
    }
    return 93;
}
#else
#include "yavo_rt.h"
#define CHECK(x) do { if (!(x)) { fprintf(stderr, "system probe failed at line %d: %s\n", __LINE__, #x); return 1; } } while (0)
static yv_str str(const char *s) { return (yv_str){s, strlen(s)}; }
static yv_string owned(const char *s) { return yv_string_from(str(s), 1, 1); }
static int child(yv_str program, const char *mode, bool diagnostics, int64_t *status, int64_t *error) {
    yv_string argument = owned(mode);
    yv_vec values = {&argument, 1, 1};
    int result = yv_process_run(program, values, diagnostics, status, error, 1, 1);
    yv_string_drop(&argument);
    return result;
}
int main(int argc, char **argv) {
    CHECK(argc == 3);
    yv_rt_init(__FILE__);
    yv_str executable = str(argv[1]), parent = str(argv[2]);
    int64_t error = 0, status = 0;
    yv_string value = {0};
    CHECK(setenv("YAVO_SYSTEM_PROBE", "", 1) == 0);
    CHECK(yv_os_env(str("YAVO_SYSTEM_PROBE"), str("wrong"), &value, &error, 1, 1) == 0 && value.len == 0);
    yv_string_drop(&value);
    CHECK(setenv("YAVO_SYSTEM_PROBE", "汉🙂", 1) == 0);
    CHECK(yv_os_env(str("YAVO_SYSTEM_PROBE"), str("wrong"), &value, &error, 1, 1) == 0);
    CHECK(value.len == (int64_t)strlen("汉🙂") && memcmp(value.ptr, "汉🙂", (size_t)value.len) == 0);
    yv_string_drop(&value);
    CHECK(setenv("YAVO_SYSTEM_PROBE", "ab\xff", 1) == 0);
    CHECK(yv_os_env(str("YAVO_SYSTEM_PROBE"), str("wrong"), &value, &error, 1, 1) == 1 && error == 2);
    yv_string_drop(&value);
    CHECK(unsetenv("YAVO_SYSTEM_PROBE") == 0);
    CHECK(yv_os_env(str("YAVO_SYSTEM_PROBE"), (yv_str){"x\0y", 3}, &value, &error, 1, 1) == 0);
    CHECK(value.len == 3 && memcmp(value.ptr, "x\0y", 3) == 0);
    yv_string_drop(&value);
    CHECK(yv_os_env(str("bad=name"), str(""), &value, &error, 1, 1) == 3);

    yv_string directory = {0}, other = {0};
    CHECK(yv_fs_temp_dir(parent, str("sys49-"), &directory, &error, 1, 1) == 0);
    CHECK(yv_fs_temp_dir(parent, str("sys49-"), &other, &error, 1, 1) == 0);
    bool same = true;
    CHECK(yv_fs_same_file(yv_string_view(&directory), yv_string_view(&other), &same, &error, 1, 1) == 0 && !same);
    CHECK(yv_fs_same_file((yv_str){NULL, 0}, (yv_str){NULL, 0}, &same, &error, 1, 1) == 0 && !same);
    yv_string file = yv_string_from(yv_string_view(&directory), 1, 1);
    yv_string_push(&file, str("/file 汉.txt"), 1, 1);
    CHECK(yv_fs_write_new_text(yv_string_view(&file), str("payload"), &error, 1, 1) == 0);
    CHECK(yv_fs_same_file(yv_string_view(&file), yv_string_view(&file), &same, &error, 1, 1) == 0 && same);
    CHECK(yv_fs_remove_dir(yv_string_view(&directory), &error, 1, 1) == 2);
    char *native = yv_path_copy(yv_string_view(&file), 1, 1);
    yv_string alias = yv_string_from(yv_string_view(&directory), 1, 1);
    yv_string_push(&alias, str("/alias"), 1, 1);
    char *alias_native = yv_path_copy(yv_string_view(&alias), 1, 1);
    CHECK(link(native, alias_native) == 0);
    CHECK(yv_fs_same_file(yv_string_view(&file), yv_string_view(&alias), &same, &error, 1, 1) == 0 && same);
    CHECK(yv_fs_remove_file(yv_string_view(&alias), &error, 1, 1) == 0);
    CHECK(symlink(native, alias_native) == 0);
    CHECK(yv_fs_same_file(yv_string_view(&file), yv_string_view(&alias), &same, &error, 1, 1) == 0 && same);
    CHECK(yv_fs_remove_file(yv_string_view(&alias), &error, 1, 1) == 0);
    free(native); free(alias_native);
    yv_string_drop(&alias);

    for (int i = 0; i < 32; i++) {
        CHECK(child(executable, "exit37", false, &status, &error) == 0 && status == 37);
        CHECK(child(executable, "exit127", false, &status, &error) == 0 && status == 127);
        CHECK(child(str("/yavo-system49-no-such-executable"), "x", false, &status, &error) == 2);
    }
    CHECK(child(executable, "signal", false, &status, &error) == 0 && status == -SIGTERM);
    CHECK(child(executable, "streams", false, &status, &error) == 0 && status == 0);
    CHECK(child(executable, "streams", true, &status, &error) == 0 && status == 0);
    const char *spelling[] = {"argv", "", "a b", "x\"y", "tail\\", "汉🙂", ";$(touch no-shell)"};
    yv_string arguments[7];
    for (int i = 0; i < 7; i++) arguments[i] = owned(spelling[i]);
    yv_vec values = {arguments, 7, 7};
    CHECK(yv_process_run(executable, values, false, &status, &error, 1, 1) == 0 && status == 0);
    for (int i = 0; i < 7; i++) yv_string_drop(&arguments[i]);
    arguments[0] = yv_string_from((yv_str){"x\0y", 3}, 1, 1);
    values.len = 1;
    CHECK(yv_process_run(executable, values, false, &status, &error, 1, 1) == 3);
    yv_string_drop(&arguments[0]);
    /* A closed stdout must not become the child's launch-error pipe. */
    int saved_stdout = dup(STDOUT_FILENO);
    CHECK(saved_stdout >= 0 && close(STDOUT_FILENO) == 0);
    int closed_result = child(str("/yavo-system49-no-such-executable"), "x", true, &status, &error);
    CHECK(dup2(saved_stdout, STDOUT_FILENO) == STDOUT_FILENO && close(saved_stdout) == 0);
    CHECK(closed_result == 2);

    CHECK(yv_fs_remove_file(yv_string_view(&file), &error, 1, 1) == 0);
    CHECK(yv_fs_remove_file(yv_string_view(&file), &error, 1, 1) == 2);
    CHECK(yv_fs_same_file(yv_string_view(&file), yv_string_view(&file), &same, &error, 1, 1) == 0 && !same);
    CHECK(yv_fs_remove_dir(yv_string_view(&directory), &error, 1, 1) == 0);
    CHECK(yv_fs_remove_dir(yv_string_view(&other), &error, 1, 1) == 0);
    CHECK(yv_fs_temp_dir(parent, str("../escape"), &value, &error, 1, 1) == 3);
    CHECK(yv_fs_same_file(str("missing"), (yv_str){"x\0y", 3}, &same, &error, 1, 1) == 3);
    yv_string_drop(&file); yv_string_drop(&directory); yv_string_drop(&other);
    CHECK(yv_rt_exit() == 0);
    fputs("system-runtime-ok\n", stdout);
    return 0;
}
#endif
