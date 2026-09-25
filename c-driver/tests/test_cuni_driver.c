/*
 * test_cuni_driver.c — exercise the C shim against a small subset of seats
 * known to emit+run locally (py, js, ts, c, cpp; NOT go — no toolchain).
 *
 * For each seat: cuni_emit -> cuni_validate, printing OK/FAIL.
 * Cross-check against the native harness's own verdict is done outside,
 * in the shell (run_tests.sh): same `cuni check --only <id>` exits.
 */
#include <stdio.h>
#include <string.h>
#include "cuni.h"

typedef struct { const char *id; const char *name; } seat_expect_t;

static const seat_expect_t SEATS[] = {
    { "py",  "Python" },
    { "js",  "JavaScript" },
    { "ts",  "TypeScript" },
    { "c",   "C" },
    { "cpp", "C++" },
};
#define N_SEATS (sizeof(SEATS) / sizeof(SEATS[0]))

int main(int argc, char **argv) {
    const char *src = (argc > 1) ? argv[1] : "examples/full.cuni";
    const char *outdir = (argc > 2) ? argv[2] : "/tmp/cuni_c_test";

    printf("cuni_version: %s\n", cuni_version());

    int n = cuni_seat_count();
    printf("cuni_seat_count: %d\n", n);
    if (n < 0) { printf("FATAL: %s\n", cuni_last_error()); return 1; }

    int fails = 0;
    for (size_t k = 0; k < N_SEATS; k++) {
        const char *id = SEATS[k].id;
        char dst[1024];
        snprintf(dst, sizeof(dst), "%s/%s.out", outdir, id);

        int er = cuni_emit(id, src, dst);
        char e_err[1024]; snprintf(e_err, sizeof(e_err), "%s", er == 0 ? "" : cuni_last_error());
        int vr = cuni_validate(id, src);
        char v_err[1024]; snprintf(v_err, sizeof(v_err), "%s", vr == 0 ? "" : cuni_last_error());

        printf("seat %-5s (%s): emit=%s validate=%s\n", id, SEATS[k].name,
               er == 0 ? "OK" : "FAIL", vr == 0 ? "OK" : "FAIL");
        if (er != 0) printf("    emit error: %s\n", e_err);
        if (vr != 0) printf("    validate error: %s\n", v_err);
        if (er != 0 || vr != 0) fails++;
    }

    /* name sanity: sample seat names by index */
    printf("--- seat_info samples ---\n");
    int idxs[] = { 0, 1, 2, 4, n - 1 };
    for (size_t k = 0; k < sizeof(idxs)/sizeof(idxs[0]); k++) {
        char nb[128];
        int r = cuni_seat_info(idxs[k], nb, sizeof(nb));
        printf("  seat_info(%d) = %s -> %s\n", idxs[k],
               r == 0 ? "OK" : "FAIL", r == 0 ? nb : cuni_last_error());
    }
    /* out-of-range must fail */
    {
        char nb[128];
        int r = cuni_seat_info(n, nb, sizeof(nb));
        printf("  seat_info(%d) [oob] = %s (expected FAIL)\n", n,
               r == 0 ? "OK" : "FAIL");
        if (r == 0) fails++;
    }

    if (fails) printf("RESULT: %d seat(s) failed\n", fails);
    else printf("RESULT: all tested seats OK\n");
    return fails ? 1 : 0;
}
