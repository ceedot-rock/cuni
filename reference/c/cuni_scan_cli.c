/*
 * Tiny CLI / test harness for ScanChunk exact-text parser.
 * Usage: cuni_scan_cli <file>
 * Prints "ok" on success, or the exact error token (e.g. CUNI_ERR_EXTRA).
 * Exit 0 on ok; 1 on CUNI_ERR_EXTRA; 2 on other parse errors.
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

    CuniScanChunk chunk;
    char err[64];
    int rc = cuni_parse_scan_chunk(text, &chunk, err, sizeof(err));
    free(text);

    if (rc == CUNI_OK) {
        printf("ok\n");
        return 0;
    }
    if (rc == CUNI_E_EXTRA) {
        /* Exact token required by cuni#17 / charggri lock */
        printf("%s\n", CUNI_ERR_EXTRA);
        return 1;
    }
    if (rc == CUNI_E_HEADER) {
        printf("CUNI_ERR_HEADER\n");
        return 2;
    }
    if (rc == CUNI_E_FORMAT) {
        printf("CUNI_ERR_FORMAT\n");
        return 2;
    }
    printf("CUNI_ERR_OTHER\n");
    return 2;
}
