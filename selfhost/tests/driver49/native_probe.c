/* Test-only argv/backend/child oracle; never linked into Yavo production code. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#ifdef _WIN32
#include <windows.h>
#include <wchar.h>
#include <io.h>
#include <fcntl.h>
#else
#include <signal.h>
#include <sys/stat.h>
#endif

static FILE *open_utf8(const char *path, const char *mode) {
#ifdef _WIN32
    int count = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, NULL, 0);
    if (!count) return NULL;
    wchar_t *wide = (wchar_t *)malloc((size_t)count * sizeof *wide);
    if (!wide) return NULL;
    if (!MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, wide, count)) {
        free(wide);
        return NULL;
    }
    FILE *file = _wfopen(wide, strcmp(mode, "rb") == 0 ? L"rb" : L"wb");
    free(wide);
    return file;
#else
    return fopen(path, mode);
#endif
}

static int record_arguments(const char *path, int argc, char **argv) {
    FILE *file = fopen(path, "wb");
    if (!file) return 60;
    for (int i = 0; i < argc; ++i) {
        size_t length = strlen(argv[i]);
        if (length > UINT32_MAX) { fclose(file); return 61; }
        unsigned char size[4];
        for (unsigned shift = 0; shift < 4; ++shift)
            size[shift] = (unsigned char)((uint32_t)length >> (8 * shift));
        if (fwrite(size, 1, 4, file) != 4 ||
            fwrite(argv[i], 1, length, file) != length) {
            fclose(file); return 62;
        }
    }
    return fclose(file) == 0 ? 0 : 63;
}

static int copy_executable(const char *source, const char *target) {
    FILE *in = open_utf8(source, "rb");
    if (!in) return 64;
    FILE *out = open_utf8(target, "wb");
    if (!out) { fclose(in); return 65; }
    unsigned char buffer[16384];
    size_t count;
    int result = 0;
    while ((count = fread(buffer, 1, sizeof buffer, in)) != 0)
        if (fwrite(buffer, 1, count, out) != count) { result = 66; break; }
    if (ferror(in)) result = 67;
    if (fclose(in) != 0) result = 68;
    if (fclose(out) != 0) result = 69;
#ifndef _WIN32
    if (!result && chmod(target, 0700) != 0) result = 70;
#endif
    return result;
}

static int leave_residue(const char *input) {
    const char *cut = strrchr(input, '/');
#ifdef _WIN32
    const char *other = strrchr(input, '\\');
    if (other && (!cut || other > cut)) cut = other;
#endif
    size_t prefix = cut ? (size_t)(cut - input + 1) : 0;
    const char name[] = "unexpected-probe-file";
    char *path = (char *)malloc(prefix + sizeof name);
    if (!path) return 71;
    memcpy(path, input, prefix);
    memcpy(path + prefix, name, sizeof name);
    FILE *out = open_utf8(path, "wb");
    free(path);
    if (!out) return 72;
    if (fwrite("retain me\n", 1, 10, out) != 10) { fclose(out); return 73; }
    return fclose(out) == 0 ? 0 : 74;
}

static int probe(int argc, char **argv) {
#ifdef _WIN32
    _setmode(_fileno(stdout), _O_BINARY);
    _setmode(_fileno(stderr), _O_BINARY);
#endif
    const char *mode = getenv("YAVO_DRIVER_PROBE_MODE");
    if (!mode) mode = "control";
    int backend = argc > 1 && strcmp(argv[1], "cc") == 0;
    int result = record_arguments(backend ? "backend-argv.bin" : "child-argv.bin", argc, argv);
    if (result) return result;
    if (strcmp(mode, "control") == 0) return 0;
    if (!backend) {
        if (strcmp(mode, "run-signal") == 0) {
#ifndef _WIN32
            raise(SIGTERM);
#endif
            return 75;
        }
        fwrite("child-out\n", 1, 10, stdout);
        fflush(stdout);
        fwrite("child-err\n", 1, 10, stderr);
        return 37;
    }

    const char *output = NULL;
    const char *input = NULL;
    for (int i = 2; i < argc; ++i) {
        if (strcmp(argv[i], "-o") == 0 && i + 1 < argc) output = argv[++i];
        else {
            size_t length = strlen(argv[i]);
            if (length >= 2 && strcmp(argv[i] + length - 2, ".c") == 0) input = argv[i];
        }
    }
    if (strcmp(mode, "missing-output") == 0) return 0;
    if (strcmp(mode, "run-signal") == 0 || strcmp(mode, "run-exit-dirty") == 0) {
        if (!output) return 76;
        result = copy_executable(argv[0], output);
        if (result) return result;
        if (strcmp(mode, "run-exit-dirty") == 0) {
            if (!input) return 77;
            result = leave_residue(input);
            if (result) return result;
        }
        return 0;
    }
    const unsigned char out[] = {'B','A','C','K','E','N','D','-','O','U','T',0xff,'\n'};
    const unsigned char err[] = {'B','A','C','K','E','N','D','-','E','R','R',0,'\n'};
    fwrite(out, 1, sizeof out, stdout);
    fflush(stdout);
    fwrite(err, 1, sizeof err, stderr);
    fflush(stderr);
    if (strcmp(mode, "dirty") == 0) {
        if (!input) return 78;
        result = leave_residue(input);
        if (result) return result;
    }
    if (strcmp(mode, "signal") == 0) {
#ifndef _WIN32
        raise(SIGTERM);
#endif
        return 79;
    }
    return 23;
}

#ifdef _WIN32
int wmain(int argc, wchar_t **wide) {
    char **argv = (char **)calloc((size_t)argc, sizeof *argv);
    if (!argv) return 80;
    int result = 0;
    for (int i = 0; i < argc; ++i) {
        int count = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide[i], -1, NULL, 0, NULL, NULL);
        if (!count) { result = 81; break; }
        argv[i] = (char *)malloc((size_t)count);
        if (!argv[i]) { result = 82; break; }
        if (!WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide[i], -1, argv[i], count, NULL, NULL)) {
            result = 83; break;
        }
    }
    if (!result) result = probe(argc, argv);
    for (int i = 0; i < argc; ++i) free(argv[i]);
    free(argv);
    return result;
}
#else
int main(int argc, char **argv) { return probe(argc, argv); }
#endif
