// The two calls src/apple/oslog.rs makes into Apple's unified logging.
//
// `os_log_with_type` is a macro, not a function: it expands to a call to
// `_os_log_impl` with the format string placed in `__TEXT,__oslogstring`, a
// section Rust has no way to emit into. So the macro is expanded here, in C,
// once per privacy class, and Rust calls these two functions instead.
//
// The format is fixed and the message is always the one `%s` argument, so a
// message containing `%` is printed as-is rather than interpreted.
//
// Public versus private is os_log's redaction: a `%{private}s` argument is
// shown as `<private>` in `log stream` and in a sysdiagnose unless the device
// carries a logging profile that reveals it. oslog.rs picks which one per
// level; see that module for the rule.

#include <os/log.h>
#include <stdint.h>

void fedimint_os_log_public(os_log_t log, uint8_t type, const char *msg) {
    os_log_with_type(log, (os_log_type_t)type, "%{public}s", msg);
}

void fedimint_os_log_private(os_log_t log, uint8_t type, const char *msg) {
    os_log_with_type(log, (os_log_type_t)type, "%{private}s", msg);
}
