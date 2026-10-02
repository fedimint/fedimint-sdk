// The calls src/apple/oslog.rs makes into Apple's unified logging.
//
// `os_log_with_type` is a macro, not a function: it expands to a call to
// `_os_log_impl` with the format string placed in `__TEXT,__oslogstring`, a
// section Rust has no way to emit into. So the macro is expanded here, in C,
// once per privacy shape, and Rust calls these three functions instead.
//
// Every format is fixed and the message only ever arrives as `%s` arguments,
// so a message containing `%` is printed as-is rather than interpreted.
//
// Public versus private is os_log's redaction: a `%{private}s` argument is
// shown as `<private>` in `log stream` and in a sysdiagnose unless the device
// carries a logging profile that reveals it. oslog.rs decides which applies
// to each piece of text; see that module for the rules.

#include <os/log.h>
#include <stdint.h>

void fedimint_os_log_public(os_log_t log, uint8_t type, const char *msg) {
    os_log_with_type(log, (os_log_type_t)type, "%{public}s", msg);
}

void fedimint_os_log_private(os_log_t log, uint8_t type, const char *msg) {
    os_log_with_type(log, (os_log_type_t)type, "%{private}s", msg);
}

// One record whose first part is public and whose second is private: the
// panic hook's shape, where the location is safe to persist and the payload,
// which can be arbitrary bytes from anywhere, is not.
void fedimint_os_log_public_private(os_log_t log, uint8_t type, const char *public_part,
                                    const char *private_part) {
    os_log_with_type(log, (os_log_type_t)type, "%{public}s%{private}s", public_part, private_part);
}
