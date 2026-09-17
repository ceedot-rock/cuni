/*
 * CuNi exact-text wire — C99 reference (not a seventh framework).
 * Rust remains the 119-lang protocol home; this refuses unknown keys
 * the same way Agent-Rider C / Chamber expect.
 *
 * Pairing: cuni#17 ↔ Agent-Rider#17 / Agent-Rider#22 (agent-rider-c SoT)
 *
 * charggri lock: the enum constant MUST be named CUNI_ERR_EXTRA (value 3).
 * Wire string via cuni_err_str is reject.extra — do not collapse the two.
 */
#ifndef CUNI_H
#define CUNI_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#define CUNI_MAX_LINE 512
#define CUNI_MAX_VAL  256

/*
 * Error enum aligned with Agent-Rider agent-rider-c/cuni.h.
 * CUNI_ERR_EXTRA == 3 is the charggri / cuni#17 locked spelling.
 */
typedef enum {
  CUNI_OK = 0,
  CUNI_ERR_EMPTY = 1,
  CUNI_ERR_KIND = 2,
  CUNI_ERR_EXTRA = 3,   /* maps to Chamber wire reject.extra — do not rename */
  CUNI_ERR_MISSING = 4,
  CUNI_ERR_HASH = 5,
  CUNI_ERR_AGENT = 6,
  CUNI_ERR_REPLAY = 7
} cuni_err;

/*
 * ScanChunk known keys (Agent-Rider#22 SoT): url, etag, hash, agent_id only.
 * Required: url, hash, agent_id. etag is optional.
 */
typedef struct {
  char url[CUNI_MAX_VAL];
  char etag[CUNI_MAX_VAL];
  char hash[CUNI_MAX_VAL];
  char agent_id[CUNI_MAX_VAL];
} scan_chunk;

#define CUNI_SCANCHUNK_BANNER "CUNI ScanChunk"

cuni_err cuni_parse_scan_chunk(const char *text, scan_chunk *out);

/* Format into buf; returns bytes written (excl NUL) or -1. */
int cuni_format_scan_chunk(const scan_chunk *in, char *buf, size_t cap);

/*
 * Wire tokens (Rider Chamber). CUNI_ERR_EXTRA → "reject.extra".
 * Enum name CUNI_ERR_EXTRA and wire reject.extra are both required; do not collapse.
 */
const char *cuni_err_str(cuni_err e);

#ifdef __cplusplus
}
#endif

#endif /* CUNI_H */
