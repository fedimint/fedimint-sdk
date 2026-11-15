import React, { useEffect, useMemo, useRef, useState } from 'react'
import { Share, StyleSheet, Text, TextInput, View } from 'react-native'
import {
  LnReceiveState_Tags,
  Notes,
  OnchainReceiveState_Tags,
  type Network,
} from '@fedimint/react-native'
import { model, satsToMsats, useDemo } from '../../model/DemoModel'
import { Button } from '../../components/Buttons'
import {
  AmountView,
  Caption,
  SatsFormat,
  StatusPill,
  TrackingStopped,
} from '../../components/Labels'
import { Card, RowDivider, Screen } from '../../components/Surfaces'
import { Field, Input, RailPicker } from '../../components/Inputs'
import { ReviewRow } from '../../components/Rows'
import { PaymentRails, type PaymentRail } from '../../components/Status'
import { CopyIconButton, middleTruncated } from '../../components/Pasteboard'
import { QRCode } from '../../components/QRCode'
import { ActivityTime } from '../../components/Activity'
import { networkName } from '../../components/Federation'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

/** 05 / 15 / 16 Receive: Lightning, Ecash and Onchain behind one control. */
export function ReceiveScreen() {
  const state = useDemo()
  const rails = PaymentRails.available(state.capabilities)
  const [rail, setRail] = useState<PaymentRail>(rails[0] ?? 'lightning')
  // Held here, not in the Lightning form, as on iOS: a new invoice being composed survives a
  // look at another rail.
  const [sats, setSats] = useState('5000')
  const [composing, setComposing] = useState(false)

  // A new invoice landing ends composing, and shows that invoice.
  useEffect(() => {
    setComposing(false)
  }, [state.lastInvoice])

  return (
    <Screen>
      <RailPicker
        rails={rails}
        selection={rail}
        onSelect={setRail}
        title={PaymentRails.title}
      />
      {rail === 'lightning' && (
        <LightningReceive
          sats={sats}
          setSats={setSats}
          composing={composing}
          setComposing={setComposing}
        />
      )}
      {rail === 'ecash' && <EcashReceive />}
      {rail === 'onchain' && <OnchainReceive />}
    </Screen>
  )
}

/**
 * The section's result line while it explains something: an error, or an ecash redeem's
 * outcome. "working…" shows as itself, since an action is under way.
 */
function ResultLine({ text }: { text?: string }) {
  if (!text) return null
  return (
    <Text selectable style={[Typeface.footnote, styles.result]}>
      {text}
    </Text>
  )
}

function LightningReceive({
  sats,
  setSats,
  composing,
  setComposing,
}: {
  sats: string
  setSats: (sats: string) => void
  composing: boolean
  setComposing: (composing: boolean) => void
}) {
  const state = useDemo()
  const amountInput = useRef<TextInput>(null)

  /**
   * An invoice is on screen unless the user has asked for a new one. While it is, the amount
   * shown is the invoice's own, read back from the SDK, and the amount field is out of the way:
   * an edited field next to an unchanged invoice would claim a different amount than the QR code
   * asks for.
   */
  const showingInvoice = state.lastInvoice !== undefined && !composing
  const msats = satsToMsats(sats)

  return (
    <View style={{ gap: Metrics.sectionGap }}>
      {showingInvoice ? (
        state.lnReceiveInvoiceMsats !== undefined && (
          <View style={{ alignItems: 'center' }}>
            <AmountView msats={state.lnReceiveInvoiceMsats} />
          </View>
        )
      ) : (
        <>
          <View style={{ alignItems: 'center', gap: 4 }}>
            <View style={styles.amountEntry}>
              <TextInput
                ref={amountInput}
                value={sats}
                onChangeText={setSats}
                keyboardType="number-pad"
                placeholder="0"
                placeholderTextColor={Palette.muted}
                style={[
                  Typeface.amount,
                  { color: Palette.ink, padding: 0, textAlign: 'right' },
                ]}
              />
              <Text style={[Typeface.amountUnit, { color: Palette.muted }]}>
                sats
              </Text>
            </View>
            <Text
              style={[Typeface.footnote, { color: Palette.muted }]}
              onPress={() => amountInput.current?.focus()}
            >
              Tap to edit
            </Text>
          </View>
          <Field label="Note (optional)">
            <Input
              value={state.lnReceiveDescription}
              onChangeText={(description) =>
                model.setLnReceive({ description })
              }
              placeholder="Coffee"
            />
          </Field>
        </>
      )}

      {showingInvoice && state.lastInvoice ? (
        <>
          <Card padding={20} style={styles.codeCard}>
            <QRCode text={state.lastInvoice} size={240} />
            <View style={styles.codeLine}>
              <Text
                style={[Typeface.mono('Regular', 13), { color: Palette.muted }]}
              >
                {middleTruncated(state.lastInvoice)}
              </Text>
              <CopyIconButton value={state.lastInvoice} />
            </View>
            <LightningStatus />
            {state.watchFailures.lnReceive && (
              <TrackingStopped reason={state.watchFailures.lnReceive} />
            )}
          </Card>
          <Button
            title="Share invoice"
            icon="square.and.arrow.up"
            onPress={() => Share.share({ message: state.lastInvoice! })}
          />
          <Button
            title="New invoice"
            kind="secondary"
            onPress={() => setComposing(true)}
          />
        </>
      ) : (
        <Button
          title="Create invoice"
          disabled={msats === undefined || state.working.has('lnReceive')}
          onPress={() => {
            if (msats === undefined) return
            model.setLnReceive({ amount: String(msats) })
            model.lnReceive()
          }}
        />
      )}
      {!showingInvoice && <ResultLine text={state.failures.lnReceive} />}
    </View>
  )
}

function LightningStatus() {
  const state = useDemo()
  const [now, setNow] = useState(Date.now())
  const waiting =
    state.lnReceiveState === undefined ||
    state.lnReceiveState.tag === LnReceiveState_Tags.Created ||
    state.lnReceiveState.tag === LnReceiveState_Tags.WaitingForPayment

  // The countdown ticks once a second while the invoice waits.
  useEffect(() => {
    if (!waiting || state.lnReceiveExpiresAt === undefined) return
    const timer = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(timer)
  }, [waiting, state.lnReceiveExpiresAt])

  if (waiting) {
    if (state.lnReceiveExpiresAt === undefined)
      return <StatusPill text="Waiting for payment" tone="warning" center />
    // "59:12" until the invoice expires, "0:00" once it has.
    const seconds = Math.max(
      0,
      Math.floor(
        (ActivityTime.date(state.lnReceiveExpiresAt).getTime() - now) / 1000,
      ),
    )
    const left = `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
    return (
      <StatusPill
        text={`Waiting for payment · ${left}`}
        tone="warning"
        center
      />
    )
  }
  switch (state.lnReceiveState?.tag) {
    case LnReceiveState_Tags.Funded:
      return <StatusPill text="Paid · claiming" tone="warning" center />
    case LnReceiveState_Tags.Claimed:
      return <StatusPill text="Received" tone="success" center />
    case LnReceiveState_Tags.Expired:
      return <StatusPill text="Expired" tone="neutral" center />
    case LnReceiveState_Tags.Canceled:
      return <StatusPill text="Canceled" tone="neutral" center />
    default:
      return <StatusPill text="Failed" tone="danger" center />
  }
}

function EcashReceive() {
  const state = useDemo()
  const text = state.ecashNotes.trim()
  /**
   * Read locally from the notes themselves: it says what they claim to be worth, not that they
   * are still unspent. Only Redeem asks the federation.
   */
  const notesValue = useMemo(() => {
    if (!text) return undefined
    try {
      return Notes.parse(text).value()
    } catch {
      return undefined
    }
  }, [text])

  return (
    <View style={{ gap: Metrics.sectionGap }}>
      <Field label="Paste ecash notes">
        <Input
          monospaced
          multiline
          value={state.ecashNotes}
          onChangeText={(notes) => model.setEcashNotes(notes)}
          style={{ minHeight: 120, textAlignVertical: 'top' }}
        />
      </Field>
      {notesValue !== undefined ? (
        <View style={{ gap: Metrics.gap }}>
          <Caption text="Preview" />
          <Card padding={0}>
            <ReviewRow
              label="Amount"
              value={SatsFormat.text(notesValue)}
              monospaced
            />
            <RowDivider />
            <ReviewRow label="Unspent" value="Checked when you redeem" />
          </Card>
        </View>
      ) : (
        text.length > 0 && (
          <Text style={[Typeface.footnote, { color: Palette.danger }]}>
            These don't look like ecash notes.
          </Text>
        )
      )}
      <Button
        title="Redeem"
        disabled={notesValue === undefined || state.working.has('ecashReceive')}
        onPress={() => model.ecashReceive()}
      />
      <ResultLine text={state.results.ecashReceive} />
    </View>
  )
}

function OnchainReceive() {
  const state = useDemo()
  const network = depositNetwork(state.federationNetwork)
  const working = state.working.has('deposit')

  return (
    <View style={{ gap: Metrics.sectionGap }}>
      {state.lastAddress ? (
        <>
          <Card padding={20} style={styles.codeCard}>
            <QRCode text={state.lastAddress} size={240} />
            <View style={styles.codeLine}>
              <Text
                style={[Typeface.mono('Regular', 13), { color: Palette.muted }]}
              >
                {middleTruncated(state.lastAddress, 10)}
              </Text>
              <CopyIconButton value={state.lastAddress} />
            </View>
            <DepositStatus />
            {state.watchFailures.deposit && (
              <TrackingStopped reason={state.watchFailures.deposit} />
            )}
          </Card>
          <Text style={[Typeface.footnote, { color: Palette.muted }]}>
            Send only {network} bitcoin to this address. Deposits can take
            several minutes to appear after they confirm.
          </Text>
          <Button
            title="Share address"
            icon="square.and.arrow.up"
            onPress={() => Share.share({ message: state.lastAddress! })}
          />
          {/* One payment per address: a second one to the same address may go unnoticed for a
              long time on the v1 wallet module. */}
          <Button
            title="New address"
            kind="secondary"
            busy={working}
            disabled={working}
            onPress={() => model.deposit()}
          />
        </>
      ) : (
        <>
          <Text style={[Typeface.callout, { color: Palette.muted }]}>
            Get an address this federation watches, then send {network} bitcoin
            to it.
          </Text>
          <Button
            title="Get deposit address"
            disabled={working}
            onPress={() => model.deposit()}
          />
        </>
      )}
      <ResultLine text={state.failures.deposit} />
    </View>
  )
}

function DepositStatus() {
  const state = useDemo()
  if (!state.depositFound)
    return <StatusPill text="Waiting for deposit" tone="warning" center />
  switch (state.depositState?.tag) {
    case undefined:
    case OnchainReceiveState_Tags.Confirmed:
      return <StatusPill text="Confirmed · claiming" tone="warning" center />
    case OnchainReceiveState_Tags.Claimed:
      return <StatusPill text="Deposited" tone="success" center />
    default:
      return <StatusPill text="Failed" tone="danger" center />
  }
}

/** The network named in the deposit copy; "the federation's" when it is not known. */
function depositNetwork(network?: Network): string {
  return network === undefined ? "the federation's" : networkName(network)
}

const styles = StyleSheet.create({
  result: { color: Palette.muted },
  amountEntry: { flexDirection: 'row', alignItems: 'baseline', gap: 8 },
  codeCard: { alignItems: 'center', gap: Metrics.gap },
  codeLine: { flexDirection: 'row', alignItems: 'center', gap: 4 },
})
