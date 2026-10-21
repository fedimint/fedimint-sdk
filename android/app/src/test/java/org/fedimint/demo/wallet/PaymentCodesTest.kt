package org.fedimint.demo.wallet

import org.fedimint.demo.ui.common.Scanned
import org.junit.Assert.assertEquals
import org.junit.Test

/** One normaliser for pasted and scanned codes, so the quote sees what the SDK parses. */
class PaymentCodesTest {
    @Test fun `schemes are stripped in any case, with a BIP21 query`() {
        assertEquals("tb1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh", PaymentCodes.strip(" bitcoin:tb1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh?amount=0.001 "))
        assertEquals("BC1QXY2KGDYGJRSQTZQ2N0YRF2493P83KKFJHX0WLH", PaymentCodes.strip("BITCOIN:BC1QXY2KGDYGJRSQTZQ2N0YRF2493P83KKFJHX0WLH"))
        assertEquals("LNBC1PVJLUEZPP5", PaymentCodes.strip("LIGHTNING:LNBC1PVJLUEZPP5"))
        assertEquals("lnbc1pvjluezpp5", PaymentCodes.strip("lnbc1pvjluezpp5"))
    }

    @Test fun `an upper-case segwit address is an address, not notes`() {
        assertEquals(
            Scanned.Address("BC1QXY2KGDYGJRSQTZQ2N0YRF2493P83KKFJHX0WLH"),
            Scanned.classify("BC1QXY2KGDYGJRSQTZQ2N0YRF2493P83KKFJHX0WLH"),
        )
        assertEquals(Scanned.Address("tb1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh"), Scanned.classify("bitcoin:tb1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh?amount=1"))
        assertEquals(Scanned.Invoice("LNBC1PVJLUEZPP5"), Scanned.classify("LIGHTNING:LNBC1PVJLUEZPP5"))
    }
}
