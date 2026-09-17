/*
 * Tiny CLI / test harness for ScanChunk exact-text parser.
 * Usage: cuni_scan_cli <file>
 * Prints wire token from cuni_err_str (ok / reject.*).
 * Exit 0 on ok; 1 on CUNI_ERR_EXTRA; 2 on other parse errors.
 *
 * Also prints enum name on stderr for golden: CUNI_ERR_EXTRA when rc==3.
 */
#include "cuni.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static char *read_file(const char *path, size_t *out_len) {
    FILE *f = fopen(path, "rb");
    if (!f) return NULL;
    if (fseek(f, 0, SEEK_END) != 0) { fclose(f); return NULL; }
    long n = ftell(f);
    if (n < 0) { fclose(f); return NULL; }
    if (fseek(f, 0, SEEK_SET) != 0) { fclose(f); return NULL; }
    char *buf = (char *)malloc((size_t)n + 1);
    if (!buf) { fclose(f); return NULL; }
    size_t got = fread(buf, 1, (size_t)n, f);
    fclose(f);
    buf[got] = '\0';
    if (out_len) *out_len = got;
    return buf;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s <scan-chunk-file>\n", argv[0]);
        return 2;
    }

    size_t len = 0;
    char *text = read_file(argv[1], &len);
    if (!text) {
        fprintf(stderr, "cannot read %s\n", argv[1]);
        return 2;
    }

    scan_chunk chunk;
    cuni_err rc = cuni_parse_scan_chunk(text, &chunk);
    free(text);

    /* stdout: Chamber wire token (reject.extra for extras) */
    printf("%s\n", cuni_err_str(rc));

    if (rc == CUNI_OK) return 0;
    if (rc == CUNI_ERR_EXTRA) {
        /* stderr: locked enum spelling for golden / charggri */
        fprintf(stderr, "CUNI_ERR_EXTRA\n");
        return 1;
    }
    return 2;
}
