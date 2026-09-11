// Stub for sdallocx to fix Android weak symbol resolution bug.
//
// aws-lc declares sdallocx as a weak symbol and checks `if (sdallocx)` before
// calling it. On Android's linker, weak undefined symbols can have their
// GLOB_DAT GOT entry resolved to the PLT stub (non-NULL) while the JUMP_SLOT
// still resolves to 0. This causes the NULL check to pass but the actual call
// to jump to address 0 (in .bss) → SIGSEGV "trying to execute non-executable
// memory".
//
// Providing a strong definition of sdallocx that delegates to free() fixes this
// by ensuring both the GOT and PLT entries resolve to a valid function.

#include <stdlib.h>

void sdallocx(void *ptr, size_t size, int flags) {
    (void)size;
    (void)flags;
    free(ptr);
}
