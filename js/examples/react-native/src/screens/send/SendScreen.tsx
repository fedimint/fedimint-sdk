import React, { useEffect, useRef, useState } from 'react'
import { Text, View } from 'react-native'
import Clipboard from '@react-native-clipboard/clipboard'
import {
  model,
  parseAmount,
  satsToMsats,
  useCloseOnAttach,
  useDemo,
  type Section,
} from '../../model/DemoModel'
import { Button } from '../../components/Buttons'
import { FailureText, SatsFormat } from '../../components/Labels'
import { Screen } from '../../components/Surfaces'
import { Field, Input, RailPicker } from '../../components/Inputs'
import { PaymentRails, type PaymentRail } from '../../components/Status'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

/** The model section a rail quotes and sends in. */
export function sendSection(rail: PaymentRail): Section {
  return rail === 'lightning'
    ? 'lnPay'
    : rail === 'ecash'
      ? 'ecashSend'
      : 'onchainSend'
}

/**
 * 14 Send: what to pay, on which rail. Nothing is spent here; "Review quote" asks the SDK what the
 * payment would cost, and the review screen executes that quote or nothing.
 */
export function SendScreen({
  review,
  close,
}: {
  review: (rail: PaymentRail) => void
  close: () => void
}) {
  const state = useDemo()
  const rails = PaymentRails.available(state.capabilities)
  const [rail, setRail] = useState<PaymentRail>(rails[0] ?? 'lightning')
  const [ecashSats, setEcashSats] = useState('1000')
  const section = sendSection(rail)

  const quoteReady =
    rail === 'lightning'
      ? state.lnQuote !== undefined
      : rail === 'ecash'
        ? state.ecashQuote !== undefined
        : state.onchainQuote !== undefined

  // The review opens when its quote lands, not when the button is tapped: a quote that fails
  // leaves the user here, with the reason. On the change only, as SwiftUI's `onChange`, and on
  // the rail it was asked for: a quote still held, from a review the user backed out of, does
  // not reopen it when Send appears or when the user switches back to its rail.
  const last = useRef({ rail, ready: quoteReady })
  useEffect(() => {
    const was = last.current
    last.current = { rail, ready: quoteReady }
    if (quoteReady && !was.ready && was.rail === rail) review(rail)
  }, [quoteReady, rail]) // eslint-disable-line react-hooks/exhaustive-deps

  useCloseOnAttach(close)

  /** The ecash amount in msats, or `undefined` for anything not a whole, positive number of sats. */
  const ecashMsats = satsToMsats(ecashSats)
  const onchainSats = parseAmount(state.onchainAmount)
  const canQuote =
    rail === 'lightning'
      ? state.lnInvoice.trim().length > 0
      : rail === 'ecash'
        ? ecashMsats !== undefined
        : state.onchainAddress.trim().length > 0 &&
          onchainSats !== undefined &&
          onchainSats > 0n
  const working = state.working.has(section)

  const requestQuote = () => {
    if (rail === 'lightning') model.lnQuoteInvoice()
    else if (rail === 'ecash') {
      if (ecashMsats === undefined) return
      model.setEcashSendAmount(String(ecashMsats))
      model.ecashQuoteAmount()
    } else model.onchainQuoteAddress()
  }

  return (
    <Screen
      footer={
        <Button
          title="Review quote"
          busy={working}
          disabled={!canQuote || working || state.isSendingEcash}
          onPress={requestQuote}
        />
      }
    >
      <RailPicker
        rails={rails}
        selection={rail}
        onSelect={setRail}
        title={PaymentRails.title}
      />

      {rail === 'lightning' && (
        <Field label="Invoice">
          <View style={{ gap: Metrics.gap }}>
            <Input
              monospaced
              multiline
              value={state.lnInvoice}
              onChangeText={(text) => model.setLnInvoice(text)}
              style={{ minHeight: 100, textAlignVertical: 'top' }}
            />
            <Button
              title="Paste"
              icon="doc.on.clipboard"
              kind="secondary"
              onPress={() =>
                Clipboard.getString().then(
                  (text) => text && model.setLnInvoice(text.trim()),
                )
              }
            />
          </View>
        </Field>
      )}

      {rail === 'ecash' && (
        <View style={{ gap: Metrics.gap }}>
          <Field label="Amount (sats)">
            <Input
              value={ecashSats}
              onChangeText={setEcashSats}
              keyboardType="number-pad"
              placeholder="1000"
            />
          </Field>
          <Text style={[Typeface.footnote, { color: Palette.muted }]}>
            Sending ecash makes notes to hand to the recipient yourself, as a
            message or a QR code.
          </Text>
        </View>
      )}

      {rail === 'onchain' && (
        <View style={{ gap: Metrics.gap }}>
          <Field label="Destination address">
            <Input
              monospaced
              value={state.onchainAddress}
              onChangeText={(address) => model.setOnchain({ address })}
              placeholder="tb1…"
            />
          </Field>
          {/* Sats, not msats: `Onchain.quote` is the one SDK call that takes whole satoshis. */}
          <Field label="Amount (sats)">
            <Input
              value={state.onchainAmount}
              onChangeText={(amount) => model.setOnchain({ amount })}
              keyboardType="number-pad"
              placeholder="10000"
            />
          </Field>
        </View>
      )}

      {state.balanceMsats !== undefined && (
        <Text
          style={[
            Typeface.mono('Regular', 13),
            { color: Palette.muted, textAlign: 'center' },
          ]}
        >
          {state.balanceFailure === undefined ? 'Available' : 'Last known'}:{' '}
          {SatsFormat.text(state.balanceMsats)}
        </Text>
      )}
      <FailureText text={state.failures[section]} />
    </Screen>
  )
}
