/*
 * CuNi exact-text wire — C99 reference (not a seventh framework).
 * Rust remains the 119-lang protocol home; this refuses unknown keys
 * the same way Agent-Rider C / Chamber expect.
 *
 * Pairing: cuni#17 ↔ Agent-Rider#17
 */
#ifndef CUNI_H
#define CUNI_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Exact token (charggri / cuni#17 lock). Maps to Rider Chamber wire reject.extra */
#define CUNI_ERR_EXTRA "CUNI_ERR_EXTRA"

#define CUNI_OK            0
#define CUNI_E_EXTRA       1  /* unknown key → emit CUNI_ERR_EXTRA */
#define CUNI_E_HEADER      2  /* missing/wrong "CUNI ScanChunk" banner */
#define CUNI_E_FORMAT      3  /* malformed key=value line */
#define CUNI_E_TRUNC       4  /* value longer than field buffer */

#define CUNI_SCANCHUNK_BANNER "CUNI ScanChunk"

/*
 * Assumed minimal ScanChunk known-key set (protocol docs lack ScanChunk keys;
 * agent-rider-c not on GitHub yet). Parallel to Rider SettleHop key=value form
 * (hop_id, job_id, …). Documented in reference/c/README.md — amend when Rider
 * lands the C tree.
 */
#define CUNI_SCANCHUNK_KEY_CHUNK_ID "chunk_id"
#define CUNI_SCANCHUNK_KEY_JOB_ID   "job_id"
#define CUNI_SCANCHUNK_KEY_AGENT_ID "agent_id"
#define CUNI_SCANCHUNK_KEY_HASH     "hash"
#define CUNI_SCANCHUNK_KEY_BODY     "body"

#define CUNI_FIELD_MAX 512

typedef struct CuniScanChunk {
    char chunk_id[CUNI_FIELD_MAX];
    char job_id[CUNI_FIELD_MAX];
    char agent_id[CUNI_FIELD_MAX];
    char hash[CUNI_FIELD_MAX];
    char body[CUNI_FIELD_MAX];
    int  has_chunk_id;
    int  has_job_id;
    int  has_agent_id;
    int  has_hash;
    int  has_body;
} CuniScanChunk;

/*
 * Parse exact-text ScanChunk:
 *   CUNI ScanChunk
 *   key=value
 *   …
 * On unknown key: returns CUNI_E_EXTRA and writes CUNI_ERR_EXTRA into
 * err_token (if non-NULL, size err_token_sz). Citizenship / exactness.passed
 * gates are unchanged — this only refuses extra keys on the wire form.
 */
int cuni_parse_scan_chunk(const char *text, CuniScanChunk *out,
                          char *err_token, size_t err_token_sz);

/* Non-zero if key is in the known ScanChunk set. */
int cuni_scanchunk_key_known(const char *key);

#ifdef __cplusplus
}
#endif

#endif /* CUNI_H */
