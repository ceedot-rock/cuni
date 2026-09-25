/*
 * cuni_driver.c — subprocess-delegation shim over the CuNi seat harness.
 *
 * New files only; nothing here modifies the tree, the registry, or any
 * tracked source. Read-only registry access: src/langs.rs is opened "r"
 * and never written.
 */
#define _POSIX_C_SOURCE 200809L

#include "cuni.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <limits.h>
#include <errno.h>
#include <fcntl.h>
#include <time.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>

/* ------------------------------------------------------------------ */
/* error buffer                                                        */
/* ------------------------------------------------------------------ */

static char last_error[1024] = "no error";

static void set_error(const char *msg) {
    if (!msg) msg = "unknown error";
    snprintf(last_error, sizeof(last_error), "%s", msg);
}

const char *cuni_last_error(void) {
    return last_error;
}

/* ------------------------------------------------------------------ */
/* paths: CUNI_TREE / CUNI_BIN, else $HOME/workspace/cuni-langs        */
/* ------------------------------------------------------------------ */

static char tree_dir[PATH_MAX + 1];
static char bin_path[PATH_MAX + 1];
static int paths_ready = 0;

/* snprintf that fails loudly on truncation instead of silently cutting */
#include <stdarg.h>
static int sn(char *dst, size_t cap, const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int r = vsnprintf(dst, cap, fmt, ap);
    va_end(ap);
    if (r < 0 || (size_t)r >= cap) {
        set_error("path too long (would truncate)");
        return -1;
    }
    return 0;
}

static int resolve_paths(void) {
    if (paths_ready) return 0;
    const char *t = getenv("CUNI_TREE");
    if (t && t[0]) {
        if (sn(tree_dir, sizeof(tree_dir), "%s", t) != 0) return -1;
    } else {
        const char *home = getenv("HOME");
        if (!home) home = "/home/hatch";
        if (sn(tree_dir, sizeof(tree_dir), "%s/workspace/cuni-langs", home) != 0) return -1;
    }
    const char *b = getenv("CUNI_BIN");
    if (b && b[0]) {
        if (sn(bin_path, sizeof(bin_path), "%s", b) != 0) return -1;
    } else {
        if (sn(bin_path, sizeof(bin_path), "%s/target/release/cuni", tree_dir) != 0) return -1;
    }
    paths_ready = 1;
    return 0;
}

/* ------------------------------------------------------------------ */
/* registry: read-only parse of src/langs.rs LANGS entries             */
/* ------------------------------------------------------------------ */

#define MAX_SEATS 1024
#define MAX_ID 64
#define MAX_NAME 128
#define MAX_EXT 32

typedef struct {
    char id[MAX_ID];
    char name[MAX_NAME];
    char ext[MAX_EXT];
} seat_t;

static seat_t seats[MAX_SEATS];
static int seat_n = -1; /* -1 = not loaded yet */

static const char *find_quoted(const char *p, char *out, size_t cap) {
    const char *q1 = strchr(p, '"');
    if (!q1) return NULL;
    q1++;
    const char *q2 = strchr(q1, '"');
    if (!q2) return NULL;
    size_t n = (size_t)(q2 - q1);
    if (n >= cap) n = cap - 1;
    memcpy(out, q1, n);
    out[n] = '\0';
    return q2 + 1;
}

/* Parse lines of the form:
 *   Lang { id: "py", name: "Python", ext: "py", family: Family::Python },
 * reading the file strictly read-only. */
static int load_registry(void) {
    if (seat_n >= 0) return seat_n;
    seat_n = 0;
    if (resolve_paths() != 0) { seat_n = -1; return -1; }

    char path[PATH_MAX + 1];
    if (sn(path, sizeof(path), "%s/src/langs.rs", tree_dir) != 0) { seat_n = -1; return -1; }
    FILE *f = fopen(path, "r");
    if (!f) {
        set_error("cannot open registry src/langs.rs (read-only)");
        seat_n = -1;
        return -1;
    }
    char line[2048];
    while (fgets(line, sizeof(line), f)) {
        if (!strstr(line, "Lang {")) continue;
        char id[MAX_ID] = {0}, name[MAX_NAME] = {0}, ext[MAX_EXT] = {0};
        const char *p = line;
        /* order in file: id, name, ext — find each key then its value */
        const char *k = strstr(p, "id:");
        if (!k || !(p = find_quoted(k, id, sizeof(id)))) continue;
        k = strstr(p, "name:");
        if (!k || !(p = find_quoted(k, name, sizeof(name)))) continue;
        k = strstr(p, "ext:");
        if (!k || !(p = find_quoted(k, ext, sizeof(ext)))) continue;
        if (seat_n < MAX_SEATS && id[0] && name[0] && ext[0]) {
            snprintf(seats[seat_n].id, sizeof(seats[seat_n].id), "%s", id);
            snprintf(seats[seat_n].name, sizeof(seats[seat_n].name), "%s", name);
            snprintf(seats[seat_n].ext, sizeof(seats[seat_n].ext), "%s", ext);
            seat_n++;
        }
    }
    fclose(f);
    if (seat_n == 0) {
        set_error("registry parsed but no Lang entries found");
        seat_n = -1;
        return -1;
    }
    return seat_n;
}

static const seat_t *find_seat(const char *seat_id) {
    if (load_registry() < 0) return NULL;
    for (int i = 0; i < seat_n; i++) {
        if (strcmp(seats[i].id, seat_id) == 0) return &seats[i];
    }
    return NULL;
}

/* ------------------------------------------------------------------ */
/* subprocess helper: run argv (NULL-terminated), capture output       */
/* ------------------------------------------------------------------ */

#define CAP_BUF 65536

static int run_capture(char *const argv[], char *out, size_t out_cap, int *exit_code) {
    int fds[2];
    if (pipe(fds) != 0) {
        set_error("pipe() failed");
        return -1;
    }
    pid_t pid = fork();
    if (pid < 0) {
        set_error("fork() failed");
        close(fds[0]); close(fds[1]);
        return -1;
    }
    if (pid == 0) {
        /* child: route stdout+stderr into the pipe, exec (no shell) */
        dup2(fds[1], STDOUT_FILENO);
        dup2(fds[1], STDERR_FILENO);
        close(fds[0]); close(fds[1]);
        execv(argv[0], argv);
        _exit(127);
    }
    close(fds[1]);
    size_t used = 0;
    for (;;) {
        if (used + 1 >= out_cap) { /* drain rest, keep tail */
            char tmp[4096];
            ssize_t r = read(fds[0], tmp, sizeof(tmp));
            if (r <= 0) break;
            continue;
        }
        ssize_t r = read(fds[0], out + used, out_cap - used - 1);
        if (r <= 0) break;
        used += (size_t)r;
    }
    out[used] = '\0';
    close(fds[0]);
    int status = 0;
    while (waitpid(pid, &status, 0) < 0 && errno == EINTR) {}
    if (WIFEXITED(status)) {
        *exit_code = WEXITSTATUS(status);
        return 0;
    }
    *exit_code = -1;
    return 0;
}

/* last non-empty line of captured output, for error text */
static void last_line(const char *cap, char *out, size_t cap_n) {
    const char *end = cap + strlen(cap);
    while (end > cap && (end[-1] == '\n' || end[-1] == '\r')) end--;
    const char *start = end;
    while (start > cap && start[-1] != '\n') start--;
    size_t n = (size_t)(end - start);
    if (n >= cap_n) n = cap_n - 1;
    memcpy(out, start, n);
    out[n] = '\0';
}

/* ------------------------------------------------------------------ */
/* small file helpers                                                  */
/* ------------------------------------------------------------------ */

static int copy_file(const char *from, const char *to) {
    FILE *in = fopen(from, "rb");
    if (!in) return -1;
    FILE *out = fopen(to, "wb");
    if (!out) { fclose(in); return -1; }
    char buf[65536];
    size_t r;
    int ok = 0;
    while ((r = fread(buf, 1, sizeof(buf), in)) > 0) {
        if (fwrite(buf, 1, r, out) != r) { ok = -1; break; }
    }
    if (ferror(in)) ok = -1;
    fclose(in);
    if (fclose(out) != 0) ok = -1;
    return ok;
}

/* recursive mkdir (like mkdir -p) */
static int mkdir_p(const char *path) {
    char tmp[PATH_MAX + 1];
    snprintf(tmp, sizeof(tmp), "%s", path);
    for (char *p = tmp + 1; *p; p++) {
        if (*p == '/') {
            *p = '\0';
            mkdir(tmp, 0755);
            *p = '/';
        }
    }
    return mkdir(tmp, 0755) == 0 || errno == EEXIST ? 0 : -1;
}

/* recursive rm -rf, used only on our own temp dirs under /tmp */
static int rm_rf(const char *path) {
    char cmd[PATH_MAX + 32];
    if (strncmp(path, "/tmp/", 5) != 0) return -1; /* safety: only /tmp */
    snprintf(cmd, sizeof(cmd), "rm -rf -- %s", path);
    return system(cmd);
}

/* ------------------------------------------------------------------ */
/* public API                                                          */
/* ------------------------------------------------------------------ */

int cuni_seat_count(void) {
    int n = load_registry();
    if (n < 0) return -1;
    return n;
}

int cuni_seat_info(int i, char *name_buf, size_t name_cap) {
    if (!name_buf || name_cap == 0) {
        set_error("cuni_seat_info: null/empty name buffer");
        return -1;
    }
    if (load_registry() < 0) return -1;
    if (i < 0 || i >= seat_n) {
        char e[128];
        snprintf(e, sizeof(e), "cuni_seat_info: seat index %d out of range (0..%d)", i, seat_n - 1);
        set_error(e);
        return -1;
    }
    size_t need = strlen(seats[i].name) + 1;
    if (need > name_cap) {
        char e[128];
        snprintf(e, sizeof(e), "cuni_seat_info: name buffer too small (%zu > %zu)", need, name_cap);
        set_error(e);
        return -1;
    }
    memcpy(name_buf, seats[i].name, need);
    return 0;
}

int cuni_emit(const char *seat_id, const char *src, const char *dst) {
    if (!seat_id || !src || !dst) {
        set_error("cuni_emit: null argument");
        return -1;
    }
    const seat_t *s = find_seat(seat_id);
    if (!s) {
        char e[160];
        snprintf(e, sizeof(e), "cuni_emit: unknown seat id `%s`", seat_id);
        set_error(e);
        return -1;
    }
    if (resolve_paths() != 0) return -1;

    char tmpd[256];
    if (sn(tmpd, sizeof(tmpd), "/tmp/cuni_emit_%d_%ld", (int)getpid(), (long)time(NULL)) != 0) return -1;

    char *argv[] = { bin_path, (char *)src, "--emit-all", tmpd, NULL };
    char cap[CAP_BUF];
    int code = 0;
    if (run_capture(argv, cap, sizeof(cap), &code) != 0) return -1;
    if (code != 0) {
        char line[512], e[768];
        last_line(cap, line, sizeof(line));
        snprintf(e, sizeof(e), "cuni --emit-all failed (exit %d): %s", code, line);
        set_error(e);
        rm_rf(tmpd);
        return -1;
    }

    char artifact[PATH_MAX + 1];
    if (sn(artifact, sizeof(artifact), "%s/%s.%s", tmpd, s->id, s->ext) != 0) { rm_rf(tmpd); return -1; }
    /* create destination dir if needed */
    char dstcopy[PATH_MAX + 1];
    snprintf(dstcopy, sizeof(dstcopy), "%s", dst);
    char *slash = strrchr(dstcopy, '/');
    if (slash) { *slash = '\0'; if (mkdir_p(dstcopy) != 0) { /* fall through, copy may still work */ } }

    if (copy_file(artifact, dst) != 0) {
        char e[768];
        snprintf(e, sizeof(e), "cuni_emit: seat `%s` emitted no artifact (expected %s.%s)", seat_id, s->id, s->ext);
        set_error(e);
        rm_rf(tmpd);
        return -1;
    }
    rm_rf(tmpd);
    return 0;
}

int cuni_validate(const char *seat_id, const char *src) {
    if (!seat_id || !src) {
        set_error("cuni_validate: null argument");
        return -1;
    }
    const seat_t *s = find_seat(seat_id);
    if (!s) {
        char e[160];
        snprintf(e, sizeof(e), "cuni_validate: unknown seat id `%s`", seat_id);
        set_error(e);
        return -1;
    }
    if (resolve_paths() != 0) return -1;

    char *argv[] = { bin_path, "check", (char *)src, "--only", (char *)seat_id, NULL };
    char cap[CAP_BUF];
    int code = 0;
    if (run_capture(argv, cap, sizeof(cap), &code) != 0) return -1;
    if (code != 0) {
        char line[512], e[768];
        last_line(cap, line, sizeof(line));
        snprintf(e, sizeof(e), "cuni check --only %s failed (exit %d): %s", seat_id, code, line);
        set_error(e);
        return -1;
    }
    return 0;
}

const char *cuni_version(void) {
    static char ver[64] = "";
    static int done = 0;
    if (done) return ver;
    done = 1;
    if (resolve_paths() != 0) return "path error";
    char *argv[] = { bin_path, "--version", NULL };
    char cap[CAP_BUF];
    int code = 0;
    snprintf(ver, sizeof(ver), "unknown");
    if (run_capture(argv, cap, sizeof(cap), &code) == 0 && code == 0) {
        /* expected: "cuni 0.1.11" */
        const char *sp = strchr(cap, ' ');
        if (sp) {
            sp++;
            size_t n = 0;
            while (sp[n] && sp[n] != '\n' && sp[n] != '\r' && n < sizeof(ver) - 1) n++;
            memcpy(ver, sp, n);
            ver[n] = '\0';
        }
    }
    return ver;
}
