/* Small Linux child meter. The measured child is forked by this low-RSS C
 * process, never by the Python report reader. stdout/stderr remain byte files. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static volatile sig_atomic_t expired;
static void timeout_signal(int sig) { (void)sig; expired = 1; }
static double seconds(struct timeval t) {
    return (double)t.tv_sec + (double)t.tv_usec / 1000000.0;
}
static double monotonic(void) {
    struct timespec t;
    if (clock_gettime(CLOCK_MONOTONIC, &t)) { perror("clock_gettime"); exit(125); }
    return (double)t.tv_sec + (double)t.tv_nsec / 1000000000.0;
}
static long rss_now(void) {
    long pages = 0, resident = 0;
    FILE *f = fopen("/proc/self/statm", "r");
    if (!f) return -1;
    int ok = fscanf(f, "%ld %ld", &pages, &resident);
    fclose(f);
    return ok == 2 ? resident * sysconf(_SC_PAGESIZE) : -1;
}
static void usage_json(FILE *f, const struct rusage *u) {
    fprintf(f, "{\"user_seconds\":%.9f,\"system_seconds\":%.9f,"
        "\"maxrss_kib\":%ld,\"ixrss\":%ld,\"idrss\":%ld,\"isrss\":%ld,"
        "\"minor_faults\":%ld,\"major_faults\":%ld,\"swaps\":%ld,"
        "\"input_blocks\":%ld,\"output_blocks\":%ld,\"messages_sent\":%ld,"
        "\"messages_received\":%ld,\"signals\":%ld,"
        "\"voluntary_context_switches\":%ld,\"involuntary_context_switches\":%ld}",
        seconds(u->ru_utime), seconds(u->ru_stime), u->ru_maxrss, u->ru_ixrss,
        u->ru_idrss, u->ru_isrss, u->ru_minflt, u->ru_majflt, u->ru_nswap,
        u->ru_inblock, u->ru_oublock, u->ru_msgsnd, u->ru_msgrcv, u->ru_nsignals,
        u->ru_nvcsw, u->ru_nivcsw);
}
int main(int argc, char **argv) {
    if (argc < 7 || argv[5][0] != '-' || argv[5][1] != '-' || argv[5][2]) {
        fprintf(stderr, "usage: perf_wait4 report stdout stderr timeout -- program [args...]\n");
        return 125;
    }
    char *end = NULL;
    unsigned long deadline = strtoul(argv[4], &end, 10);
    if (!end || *end || !deadline || deadline > 7200) return 125;
    int out = open(argv[2], O_WRONLY | O_CREAT | O_TRUNC, 0600);
    int err = open(argv[3], O_WRONLY | O_CREAT | O_TRUNC, 0600);
    int in = open("/dev/null", O_RDONLY);
    if (out < 0 || err < 0 || in < 0) { perror("open"); return 125; }
    struct rusage before = {0}, child_usage = {0}, after = {0};
    getrusage(RUSAGE_SELF, &before);
    long parent_rss = rss_now();
    struct sigaction action = {0};
    action.sa_handler = timeout_signal;
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGALRM, &action, NULL)) return 125;
    double started = monotonic();
    pid_t child = fork();
    if (child < 0) { perror("fork"); return 125; }
    if (!child) {
        if (setpgid(0, 0) || dup2(in, STDIN_FILENO) < 0 || dup2(out, STDOUT_FILENO) < 0
            || dup2(err, STDERR_FILENO) < 0) _exit(126);
        close(in); close(out); close(err);
        execvp(argv[6], argv + 6);
        perror("execvp");
        _exit(127);
    }
    close(in); close(out); close(err);
    (void)setpgid(child, child);
    alarm((unsigned)deadline);
    int status = 0;
    for (;;) {
        pid_t got = wait4(child, &status, 0, &child_usage);
        if (got == child) break;
        if (got < 0 && errno == EINTR) {
            if (expired) (void)kill(-child, SIGKILL);
            continue;
        }
        perror("wait4");
        (void)kill(-child, SIGKILL);
        return 125;
    }
    alarm(0);
    double elapsed = monotonic() - started;
    getrusage(RUSAGE_SELF, &after);
    int code = WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
    FILE *f = fopen(argv[1], "w");
    if (!f) { perror("report"); return 125; }
    fprintf(f, "{\"schema\":\"yavo-linux-wait4/1\",\"pid\":%ld,"
        "\"raw_wait_status\":%d,\"exit_code\":%d,\"signal\":%d,"
        "\"timed_out\":%s,\"elapsed_seconds\":%.9f,"
        "\"supervisor_rss_before_fork_bytes\":%ld,\"rusage\":",
        (long)child, status, code, WIFSIGNALED(status) ? WTERMSIG(status) : 0,
        expired ? "true" : "false", elapsed, parent_rss);
    usage_json(f, &child_usage);
    fputs(",\"supervisor_before\":", f); usage_json(f, &before);
    fputs(",\"supervisor_after\":", f); usage_json(f, &after);
    fputs("}\n", f);
    if (fclose(f)) return 125;
    return code;
}
