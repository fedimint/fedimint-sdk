import React, { useEffect, useState } from 'react'
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native'
import { Icon } from '../../components/Icon'
import type { LightningRoute } from '@fedimint/react-native'
import { model, useDemo } from '../../model/DemoModel'
import { Button } from '../../components/Buttons'
import {
  AmountView,
  Caption,
  SatsFormat,
  StatusPill,
} from '../../components/Labels'
import { Card, RowDivider, Screen } from '../../components/Surfaces'
import { ReviewRow } from '../../components/Rows'
import { PaymentRails, type PaymentRail } from '../../components/Status'
import { middleTruncated } from '../../components/Pasteboard'
import { ActivityTime } from '../../components/Activity'
import { routeLabel } from '../activity/OperationDetailScreen'
import { sendSection } from './SendScreen'
import { SendResult } from './SendResult'
import { EcashReady } from './EcashReady'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

/** One quote's figures, whichever rail it came from. */
interface Terms {
  amount: bigint
  notesValue?: bigint
  fee: bigint
  total: bigint
  expiresAt: bigint
  destination?: string
  route?: LightningRoute
}

/**
 * 06 Review: the quote the SDK returned, exactly what executing it will cost, and the time it
 * stays valid. Approving executes this quote and nothing else; the SDK refuses one that has
 * expired or changed underneath it.
 */
export function ReviewScreen({
  rail,
  done,
  back,
  setSubmitted,
  submitted,
}: {
  rail: PaymentRail
  done: () => void
  back: () => void
  submitted: boolean
  setSubmitted: () => void
}) {
  const state = useDemo()
  const section = sendSection(rail)
  const [now, setNow] = useState(Date.now())

  // The quote's countdown, until it is approved: the result shown after needs no clock.
  useEffect(() => {
    if (submitted) return
    const timer = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(timer)
  }, [submitted])

  if (submitted) {
    return rail === 'ecash' ? (
      <EcashReady done={done} />
    ) : (
      <SendResult rail={rail} done={done} />
    )
  }

  let terms: Terms | undefined
  if (rail === 'lightning' && state.lnQuote) {
    const quote = state.lnQuote
    terms = {
      amount: quote.invoiceAmount(),
      fee: quote.fee(),
      total: quote.total(),
      expiresAt: quote.expiresAt(),
      destination: state.lnInvoice.trim(),
      route: quote.route(),
    }
  } else if (rail === 'ecash' && state.ecashQuote) {
    const quote = state.ecashQuote
    // A mint rounds a request up to the notes it can issue, so the notes can be worth more than
    // asked for; the review says so.
    const notes = quote.notesValue()
    terms = {
      amount: quote.requestedAmount(),
      notesValue: notes === quote.requestedAmount() ? undefined : notes,
      fee: quote.fee(),
      total: quote.total(),
      expiresAt: quote.expiresAt(),
    }
  } else if (rail === 'onchain' && state.onchainQuote) {
    const quote = state.onchainQuote
    // Whole sats on chain, msats for the fee and total: the units the SDK reports each in.
    terms = {
      amount: quote.amount() * 1000n,
      fee: quote.fee(),
      total: quote.total(),
      expiresAt: quote.expiresAt(),
      destination: state.onchainAddress.trim(),
    }
  }

  // No quote to review: a new one is on its way, the last request for one failed, or it has been
  // used or dropped.
  if (!terms) {
    if (state.working.has(section)) {
      return (
        <View style={styles.centered}>
          <ActivityIndicator />
          <Text style={[Typeface.callout, { color: Palette.muted }]}>
            Getting a new quote
          </Text>
        </View>
      )
    }
    return (
      <View style={[styles.centered, { gap: Metrics.gap }]}>
        <Text
          selectable
          style={[
            Typeface.callout,
            { color: Palette.muted, textAlign: 'center' },
          ]}
        >
          {state.failures[section] ?? 'This quote is no longer available.'}
        </Text>
        <Button title="Back" kind="secondary" onPress={back} />
      </View>
    )
  }

  const secondsLeft = Math.floor(
    (ActivityTime.date(terms.expiresAt).getTime() - now) / 1000,
  )

  /**
   * Moves on to the result only if the send started: one refused at the start, such as while a
   * federation is being attached, has no result of its own to show. Never moves back.
   */
  const approve = () => {
    const started =
      rail === 'lightning'
        ? model.lnPay()
        : rail === 'ecash'
          ? model.ecashSend()
          : model.onchainSend()
    if (started) setSubmitted()
  }
  const requote = () => {
    if (rail === 'lightning') model.lnQuoteInvoice()
    else if (rail === 'ecash') model.ecashQuoteAmount()
    else model.onchainQuoteAddress()
  }

  return (
    <Screen
      footer={
        <>
          {secondsLeft > 0 ? (
            <Button
              title="Approve & send"
              disabled={
                model.isAttaching || (rail === 'ecash' && state.isSendingEcash)
              }
              onPress={approve}
            />
          ) : (
            <Button
              title="Get a new quote"
              disabled={state.working.has(section)}
              onPress={requote}
            />
          )}
          <Button
            title="Cancel"
            kind="secondary"
            onPress={() => {
              model.discardQuotes()
              back()
            }}
          />
        </>
      }
    >
      <View style={{ gap: 8 }}>
        <Caption text="You're sending" />
        <AmountView msats={terms.amount} />
      </View>
      <Card padding={0}>
        {terms.destination && (
          <>
            <ReviewRow
              label="To"
              value={middleTruncated(terms.destination, 10)}
              monospaced
            />
            <RowDivider />
          </>
        )}
        <ReviewRow label="Via" value={PaymentRails.title(rail)} />
        {terms.route && (
          <>
            <RowDivider />
            <ReviewRow
              label="Route"
              value={routeLabel(terms.route)}
              monospaced
            />
          </>
        )}
        {terms.notesValue !== undefined && (
          <>
            <RowDivider />
            <ReviewRow
              label="Notes worth"
              value={SatsFormat.text(terms.notesValue)}
              monospaced
            />
          </>
        )}
        <RowDivider />
        {/* "Fee", not "network fee": it is every cost of the payment, the gateway's and the
            federation's included. */}
        <ReviewRow label="Fee" value={SatsFormat.text(terms.fee)} monospaced />
        <RowDivider />
        <ReviewRow
          label="Total"
          value={SatsFormat.text(terms.total)}
          monospaced
          emphasised
        />
      </Card>
      {secondsLeft > 0 ? (
        <View style={styles.locked}>
          <Icon name="lock" size={13} weight="regular" color={Palette.muted} />
          <Text
            style={[Typeface.mono('Regular', 13), { color: Palette.muted }]}
          >
            Quote locked · expires in {Math.floor(secondsLeft / 60)}:
            {String(secondsLeft % 60).padStart(2, '0')}
          </Text>
        </View>
      ) : (
        <StatusPill
          text="Quote expired · get a new one to send"
          tone="warning"
        />
      )}
    </Screen>
  )
}

const styles = StyleSheet.create({
  centered: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    padding: Metrics.screenPadding,
    gap: 8,
    backgroundColor: Palette.background,
  },
  locked: { flexDirection: 'row', alignItems: 'center', gap: 6 },
})
