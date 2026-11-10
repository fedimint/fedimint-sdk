import React from 'react'
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native'
import { useSafeAreaInsets } from 'react-native-safe-area-context'
import { Icon, type IconName } from '../../components/Icon'
import {
  LnSendState_Tags,
  OnchainSendState_Tags,
  type LightningRoute,
} from '@fedimint/react-native'
import { useDemo } from '../../model/DemoModel'
import { sendSection } from './SendScreen'
import { Button } from '../../components/Buttons'
import {
  Caption,
  SatsFormat,
  toneColors,
  type Tone,
} from '../../components/Labels'
import { Card } from '../../components/Surfaces'
import { CopyIconButton, middleTruncated } from '../../components/Pasteboard'
import type { PaymentRail } from '../../components/Status'
import { routeLabel } from '../activity/OperationDetailScreen'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

/**
 * After a Lightning or Onchain send: the payment's state as the SDK reports it, until it is final.
 * The SDK's rule shows here: an error means the call failed, while a payment that did not go
 * through is a state. So `refunded` reads as an outcome, money back in the balance, not a failure.
 */
export function SendResult({
  rail,
  done,
}: {
  rail: PaymentRail
  done: () => void
}) {
  const state = useDemo()
  const insets = useSafeAreaInsets()
  const section = sendSection(rail)
  const failure = state.failures[section]
  const lost = state.watchFailures[section]

  let outcome: React.ReactNode
  if (failure) {
    // The call itself failed, so there may be no payment at all. The SDK's reason as it came.
    outcome = (
      <Outcome
        icon="exclamationmark.triangle"
        tone="danger"
        title="Couldn't send"
        detail={failure}
      />
    )
  } else if (lost) {
    // The send was submitted, and then reading its state failed. That says nothing about how the
    // payment ended, so neither outcome is claimed.
    outcome = (
      <Outcome
        icon="questionmark"
        tone="neutral"
        title="Status unknown"
        detail={`The payment was submitted, but the SDK stopped reporting its state: ${lost}. Check your balance and Activity to see how it ended.`}
      />
    )
  } else if (rail === 'lightning') {
    const s = state.lnSendState
    switch (s?.tag) {
      case undefined:
      case LnSendState_Tags.Created:
      case LnSendState_Tags.Funded:
        outcome = (
          <Outcome
            icon="bolt"
            tone="warning"
            title="Sending"
            detail="Waiting for the payment to settle."
            busy
          />
        )
        break
      case LnSendState_Tags.Success: {
        const inner = s.inner as {
          preimage: string
          fee: bigint
          route: LightningRoute
        }
        outcome = (
          <Outcome
            icon="checkmark"
            tone="success"
            title="Sent"
            detail={`Fee ${SatsFormat.text(inner.fee)} · ${routeLabel(inner.route)}`}
            extra={
              <Receipt label="Receipt (preimage)" value={inner.preimage} />
            }
          />
        )
        break
      }
      case LnSendState_Tags.Refunded:
        outcome = (
          <Outcome
            icon="arrow.uturn.backward"
            tone="neutral"
            title="Not sent"
            detail="The payment didn't go through. The amount is back in your balance."
          />
        )
        break
      default:
        outcome = (
          <Outcome
            icon="xmark"
            tone="danger"
            title="Payment failed"
            detail={(s?.inner as { reason: string }).reason}
          />
        )
    }
  } else {
    const s = state.onchainSendState
    switch (s?.tag) {
      case undefined:
      case OnchainSendState_Tags.Created:
        outcome = (
          <Outcome
            icon="link"
            tone="warning"
            title="Sending"
            detail="The federation is building and signing the transaction."
            busy
          />
        )
        break
      case OnchainSendState_Tags.Succeeded:
        outcome = (
          <Outcome
            icon="checkmark"
            tone="success"
            title="Sent"
            detail="The transaction is on its way."
            extra={
              <Receipt
                label="Transaction"
                value={(s.inner as { txid: string }).txid}
              />
            }
          />
        )
        break
      case OnchainSendState_Tags.Refunded:
        outcome = (
          <Outcome
            icon="arrow.uturn.backward"
            tone="neutral"
            title="Not sent"
            detail={`The amount is back in your balance. ${(s.inner as { reason: string }).reason}`}
          />
        )
        break
      default:
        outcome = (
          <Outcome
            icon="xmark"
            tone="danger"
            title="Withdrawal failed"
            detail={(s?.inner as { reason: string }).reason}
          />
        )
    }
  }

  return (
    <View style={styles.screen}>
      <View style={styles.center}>{outcome}</View>
      <View
        style={{
          padding: Metrics.screenPadding,
          paddingBottom: Math.max(insets.bottom, Metrics.screenPadding),
        }}
      >
        <Button title="Done" onPress={done} />
      </View>
    </View>
  )
}

/** The large icon, title and explanation an outcome screen is built from. */
function Outcome({
  icon,
  tone,
  title,
  detail,
  busy = false,
  extra,
}: {
  icon: IconName
  tone: Tone
  title: string
  detail: string
  busy?: boolean
  extra?: React.ReactNode
}) {
  return (
    <View style={styles.outcome}>
      <View style={[styles.icon, { backgroundColor: toneColors[tone].tint }]}>
        <Icon
          name={icon}
          size={26}
          weight="semibold"
          color={toneColors[tone].fg}
        />
      </View>
      <Text style={[Typeface.title, { color: Palette.ink }]}>{title}</Text>
      <Text
        selectable
        style={[
          Typeface.callout,
          { color: Palette.muted, textAlign: 'center' },
        ]}
      >
        {detail}
      </Text>
      {busy && <ActivityIndicator />}
      {extra}
    </View>
  )
}

/** A long value worth keeping, a preimage or a txid, shortened with a copy. */
function Receipt({ label, value }: { label: string; value: string }) {
  return (
    <Card style={{ alignItems: 'center', gap: 6, marginTop: Metrics.gap }}>
      <Caption text={label} />
      <View style={{ flexDirection: 'row', alignItems: 'center', gap: 4 }}>
        <Text style={[Typeface.mono('Regular', 13), { color: Palette.ink }]}>
          {middleTruncated(value, 10)}
        </Text>
        <CopyIconButton value={value} />
      </View>
    </Card>
  )
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: Palette.background },
  center: { flex: 1, justifyContent: 'center', padding: Metrics.screenPadding },
  outcome: { alignItems: 'center', gap: Metrics.gap },
  icon: {
    width: 64,
    height: 64,
    borderRadius: 32,
    alignItems: 'center',
    justifyContent: 'center',
  },
})
