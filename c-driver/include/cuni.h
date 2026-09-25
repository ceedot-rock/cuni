/*
 * cuni.h — C API shim over CuNi's seat harness.
 *
 * This is NOT a native rewrite of CuNi. It is a thin driver that delegates to
 * the existing tooling by subprocess:
 *   - seat registry: parsed read-only from src/langs.rs (the LANGS array)
 *   - emit:         `cuni <src> --emit-all <tmpdir>` then copy <tmpdir>/<id>.<ext>
 *   - validate:     `cuni check <src> --only <seat_id>`
 *
 * A call returns 0 only when the underlying harness reports success.
 * On failure, cuni_last_error() holds a short diagnostic.
 *
 * Threading: not thread-safe (static error buffer + cached registry).
 */
#ifndef CUNI_DRIVER_H
#define CUNI_DRIVER_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Number of seats in the registry (parsed read-only from src/langs.rs).
 * Returns -1 and sets cuni_last_error() if the registry cannot be read. */
int cuni_seat_count(void);

/* Copy seat i's display name into name_buf (NUL-terminated).
 * 0 on success; -1 if i is out of range, name_buf is NULL/too small,
 * or the registry cannot be read. */
int cuni_seat_info(int i, char *name_buf, size_t name_cap);

/* Emit seat_id's code for the CuNi program at src, writing it to dst.
 * Delegates to `cuni <src> --emit-all <tmpdir>` and copies that seat's
 * artifact. 0 on success; -1 otherwise (see cuni_last_error()). */
int cuni_emit(const char *seat_id, const char *src, const char *dst);

/* Validate seat_id's code for the CuNi program at src.
 * Delegates to `cuni check <src> --only <seat_id>`.
 * 0 on success; -1 otherwise (see cuni_last_error()). */
int cuni_validate(const char *seat_id, const char *src);

/* Short human-readable diagnostic for the most recent failure. Never NULL. */
const char *cuni_last_error(void);

/* Tooling version (from `cuni --version`), or "unknown" if the binary
 * cannot be queried. Never NULL. */
const char *cuni_version(void);

#ifdef __cplusplus
}
#endif

#endif /* CUNI_DRIVER_H */
