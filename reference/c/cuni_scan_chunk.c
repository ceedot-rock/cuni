#define _POSIX_C_SOURCE 200809L
/*
 * C99 reference parser for CuNi ScanChunk exact-text wire form.
 * Adapted from Agent-Rider agent-rider-c/cuni.c (PR #22 / feat/sitescan-c99-rider).
 *
 * Extra key / malformed line without '=' → CUNI_ERR_EXTRA (wire reject.extra).
 * Missing required url/hash/agent_id → CUNI_ERR_MISSING.
 */
#include "cuni.h"

#include <ctype.h>
#include <stdio.h>
#include <string.h>

const char *cuni_err_str(cuni_err e) {
  switch (e) {
    case CUNI_OK: return "ok";
    case CUNI_ERR_EMPTY: return "reject.empty";
    case CUNI_ERR_KIND: return "reject.kind";
    case CUNI_ERR_EXTRA: return "reject.extra";
    case CUNI_ERR_MISSING: return "reject.missing";
    case CUNI_ERR_HASH: return "reject.hash";
    case CUNI_ERR_AGENT: return "reject.agent";
    case CUNI_ERR_REPLAY: return "reject.replay";
    default: return "reject.unknown";
  }
}

static void trim(char *s) {
  size_t n = strlen(s);
  while (n && (s[n - 1] == '\n' || s[n - 1] == '\r' || isspace((unsigned char)s[n - 1])))
    s[--n] = 0;
  char *p = s;
  while (*p && isspace((unsigned char)*p)) p++;
  if (p != s) memmove(s, p, strlen(p) + 1);
}

/* allowed: list of key names ending with NULL. Returns CUNI_ERR_EXTRA on unknown key
 * or malformed line without '='. */
static cuni_err parse_kv(const char *text, const char *kind,
                         const char **allowed,
                         int (*set)(const char *k, const char *v, void *ctx),
                         void *ctx) {
  if (!text || !*text) return CUNI_ERR_EMPTY;
  char tmp[8192];
  if (strlen(text) >= sizeof(tmp)) return CUNI_ERR_EMPTY;
  memcpy(tmp, text, strlen(text) + 1);

  char *save = NULL;
  char *line = strtok_r(tmp, "\n", &save);
  if (!line) return CUNI_ERR_EMPTY;
  trim(line);
  char expect[64];
  snprintf(expect, sizeof(expect), "CUNI %s", kind);
  if (strcmp(line, expect) != 0) return CUNI_ERR_KIND;

  int saw = 0;
  while ((line = strtok_r(NULL, "\n", &save)) != NULL) {
    trim(line);
    if (!*line) continue;
    char *eq = strchr(line, '=');
    if (!eq) return CUNI_ERR_EXTRA;
    *eq = 0;
    char *k = line;
    char *v = eq + 1;
    trim(k); trim(v);
    int ok = 0;
    for (const char **a = allowed; *a; a++) {
      if (strcmp(k, *a) == 0) { ok = 1; break; }
    }
    if (!ok) return CUNI_ERR_EXTRA;
    if (set(k, v, ctx) != 0) return CUNI_ERR_EXTRA;
    saw++;
  }
  if (!saw) return CUNI_ERR_EMPTY;
  return CUNI_OK;
}

static int set_scan(const char *k, const char *v, void *ctx) {
  scan_chunk *o = ctx;
  if (strcmp(k, "url") == 0) { snprintf(o->url, sizeof(o->url), "%s", v); return 0; }
  if (strcmp(k, "etag") == 0) { snprintf(o->etag, sizeof(o->etag), "%s", v); return 0; }
  if (strcmp(k, "hash") == 0) { snprintf(o->hash, sizeof(o->hash), "%s", v); return 0; }
  if (strcmp(k, "agent_id") == 0) { snprintf(o->agent_id, sizeof(o->agent_id), "%s", v); return 0; }
  return -1;
}

cuni_err cuni_parse_scan_chunk(const char *text, scan_chunk *out) {
  memset(out, 0, sizeof(*out));
  static const char *allow[] = {"url", "etag", "hash", "agent_id", NULL};
  cuni_err e = parse_kv(text, "ScanChunk", allow, set_scan, out);
  if (e != CUNI_OK) return e;
  if (!out->url[0] || !out->hash[0] || !out->agent_id[0]) return CUNI_ERR_MISSING;
  return CUNI_OK;
}

int cuni_format_scan_chunk(const scan_chunk *in, char *buf, size_t cap) {
  return snprintf(buf, cap,
    "CUNI ScanChunk\nurl=%s\netag=%s\nhash=%s\nagent_id=%s\n",
    in->url, in->etag, in->hash, in->agent_id);
}
