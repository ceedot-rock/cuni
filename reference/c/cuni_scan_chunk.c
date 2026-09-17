/*
 * C99 reference parser for CuNi ScanChunk exact-text wire form.
 * Unknown keys → CUNI_ERR_EXTRA (↔ Rider Chamber reject.extra).
 */
#include "cuni.h"

#include <string.h>
#include <ctype.h>

static void set_err(char *buf, size_t n, const char *tok) {
    if (!buf || n == 0) return;
    size_t i = 0;
    while (tok[i] && i + 1 < n) {
        buf[i] = tok[i];
        i++;
    }
    buf[i] = '\0';
}

static void trim_inplace(char *s) {
    char *start = s;
    while (*start && isspace((unsigned char)*start)) start++;
    if (start != s) memmove(s, start, strlen(start) + 1);
    size_t len = strlen(s);
    while (len > 0 && isspace((unsigned char)s[len - 1])) {
        s[len - 1] = '\0';
        len--;
    }
}

int cuni_scanchunk_key_known(const char *key) {
    if (!key) return 0;
    return strcmp(key, CUNI_SCANCHUNK_KEY_CHUNK_ID) == 0
        || strcmp(key, CUNI_SCANCHUNK_KEY_JOB_ID) == 0
        || strcmp(key, CUNI_SCANCHUNK_KEY_AGENT_ID) == 0
        || strcmp(key, CUNI_SCANCHUNK_KEY_HASH) == 0
        || strcmp(key, CUNI_SCANCHUNK_KEY_BODY) == 0;
}

static int copy_field(char *dst, size_t dstsz, const char *val) {
    size_t i = 0;
    if (dstsz == 0) return CUNI_E_TRUNC;
    while (val[i] && i + 1 < dstsz) {
        dst[i] = val[i];
        i++;
    }
    if (val[i] != '\0') {
        dst[0] = '\0';
        return CUNI_E_TRUNC;
    }
    dst[i] = '\0';
    return CUNI_OK;
}

int cuni_parse_scan_chunk(const char *text, CuniScanChunk *out,
                          char *err_token, size_t err_token_sz) {
    if (err_token && err_token_sz) err_token[0] = '\0';
    if (!text || !out) return CUNI_E_FORMAT;

    memset(out, 0, sizeof(*out));

    /* First non-empty line must be exact banner (allow trailing CR). */
    const char *p = text;
    while (*p == '\n' || *p == '\r') p++;

    const char *line_end = p;
    while (*line_end && *line_end != '\n' && *line_end != '\r') line_end++;

    {
        size_t blen = (size_t)(line_end - p);
        size_t banlen = strlen(CUNI_SCANCHUNK_BANNER);
        if (blen != banlen || strncmp(p, CUNI_SCANCHUNK_BANNER, banlen) != 0) {
            return CUNI_E_HEADER;
        }
    }

    p = line_end;
    if (*p == '\r') p++;
    if (*p == '\n') p++;

    while (*p) {
        const char *ls = p;
        while (*p && *p != '\n' && *p != '\r') p++;
        size_t llen = (size_t)(p - ls);

        /* advance past newline */
        if (*p == '\r') p++;
        if (*p == '\n') p++;

        if (llen == 0) continue; /* blank line ok */

        char line[CUNI_FIELD_MAX * 2];
        if (llen >= sizeof(line)) return CUNI_E_TRUNC;
        memcpy(line, ls, llen);
        line[llen] = '\0';
        trim_inplace(line);
        if (line[0] == '\0') continue;
        if (line[0] == '#') continue; /* comment lines ignored */

        char *eq = strchr(line, '=');
        if (!eq) return CUNI_E_FORMAT;
        *eq = '\0';
        char *key = line;
        char *val = eq + 1;
        trim_inplace(key);
        /* values keep interior spaces; trim ends only */
        trim_inplace(val);

        if (key[0] == '\0') return CUNI_E_FORMAT;

        if (!cuni_scanchunk_key_known(key)) {
            set_err(err_token, err_token_sz, CUNI_ERR_EXTRA);
            return CUNI_E_EXTRA;
        }

        if (strcmp(key, CUNI_SCANCHUNK_KEY_CHUNK_ID) == 0) {
            if (copy_field(out->chunk_id, sizeof(out->chunk_id), val) != CUNI_OK)
                return CUNI_E_TRUNC;
            out->has_chunk_id = 1;
        } else if (strcmp(key, CUNI_SCANCHUNK_KEY_JOB_ID) == 0) {
            if (copy_field(out->job_id, sizeof(out->job_id), val) != CUNI_OK)
                return CUNI_E_TRUNC;
            out->has_job_id = 1;
        } else if (strcmp(key, CUNI_SCANCHUNK_KEY_AGENT_ID) == 0) {
            if (copy_field(out->agent_id, sizeof(out->agent_id), val) != CUNI_OK)
                return CUNI_E_TRUNC;
            out->has_agent_id = 1;
        } else if (strcmp(key, CUNI_SCANCHUNK_KEY_HASH) == 0) {
            if (copy_field(out->hash, sizeof(out->hash), val) != CUNI_OK)
                return CUNI_E_TRUNC;
            out->has_hash = 1;
        } else if (strcmp(key, CUNI_SCANCHUNK_KEY_BODY) == 0) {
            if (copy_field(out->body, sizeof(out->body), val) != CUNI_OK)
                return CUNI_E_TRUNC;
            out->has_body = 1;
        }
    }

    return CUNI_OK;
}
