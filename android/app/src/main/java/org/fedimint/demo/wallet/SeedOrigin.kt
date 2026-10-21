package org.fedimint.demo.wallet

/**
 * Where this wallet's seed came from, which decides how it should join a
 * federation.
 *
 * A seed restored from a phrase may already own funds in a federation, and
 * only `recover()` finds them; a plain `join()` can never be turned into a
 * recovery later. So only a seed known to have been created on this device is
 * joined plainly by default. A missing record ([UNKNOWN]: an install from
 * before the record existed, or one interrupted before writing it) is never
 * taken as proof the seed is fresh.
 */
enum class SeedOrigin {
    CREATED,
    RESTORED,
    UNKNOWN;

    /** Whether joining should default to recovering. Plain join stays available either way. */
    val recoverByDefault: Boolean get() = this != CREATED

    companion object {
        /**
         * Reads the stored record. [legacyRestored] is the boolean an earlier
         * build wrote only for restores; it never wrote "created", so its
         * absence means unknown.
         */
        fun fromStored(value: String?, legacyRestored: Boolean): SeedOrigin = when {
            value == CREATED.name -> CREATED
            value == RESTORED.name || legacyRestored -> RESTORED
            else -> UNKNOWN
        }
    }
}
