package org.fedimint.demo.ui.payments

/** The three payment modules a federation may run, each named for what the user sees. */
enum class Rail(val title: String) { Lightning("Lightning"), Ecash("Ecash"), Onchain("On-chain") }

enum class PaymentDirection { Send, Receive }
