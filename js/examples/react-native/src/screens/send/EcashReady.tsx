import React from 'react'
import { ActivityIndicator, Share, Text, View } from 'react-native'
import { EcashSendState } from '@fedimint/react-native'
import { model, useDemo } from '../../model/DemoModel'
import { Button, TextButton } from '../../components/Buttons'
import {
  AmountView,
  StatusPill,
  TrackingStopped,
} from '../../components/Labels'
import { Card, Screen } from '../../components/Surfaces'
import { CopyIconButton, middleTruncated } from '../../components/Pasteboard'
import { QRCode } from '../../components/QRCode'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

/**
 * 17 Ecash ready: the notes an ecash send made, to hand to the recipient, and what has become of
 * them since. The notes are bearer value: whoever redeems them first gets the money. So they
 * leave this screen only deliberately, through the copy and share buttons.
 */
export function EcashReady({ done }: { done: () => void }) {
  const state = useDemo()
  const notes = state.lastNotes
  const token = notes?.display()
  const sendState = state.ecashSendState
  /** Reclaiming is a request the receiver's redeem can still beat: only while outstanding. */
  const canReclaim =
    (sendState === undefined || sendState === EcashSendState.Created) &&
    !state.working.has('ecashReclaim')

  return (
    <Screen
      footer={
        <>
          {token && (
            <>
              <Button
                title="Share notes"
                icon="square.and.arrow.up"
                onPress={() => Share.share({ message: token })}
              />
              <Button
                title="Reclaim notes"
                kind="secondary"
                disabled={!canReclaim}
                onPress={() => model.reclaimEcash()}
              />
            </>
          )}
          <TextButton title="Done" onPress={done} />
        </>
      }
    >
      {notes && token ? (
        <>
          <View style={{ alignItems: 'center', gap: 4 }}>
            <AmountView msats={notes.value()} />
            <Text style={[Typeface.footnote, { color: Palette.muted }]}>
              From {state.federationName ?? 'this federation'}
            </Text>
          </View>
          <Card padding={20} style={{ alignItems: 'center', gap: Metrics.gap }}>
            <QRCode text={token} size={240} />
            <View
              style={{ flexDirection: 'row', alignItems: 'center', gap: 4 }}
            >
              <Text
                style={[Typeface.mono('Regular', 13), { color: Palette.muted }]}
              >
                {middleTruncated(token, 12)}
              </Text>
              <CopyIconButton value={token} bearer />
            </View>
            <EcashStatus state={sendState} />
            {state.watchFailures.ecashSend && (
              <TrackingStopped reason={state.watchFailures.ecashSend} />
            )}
          </Card>
          <Text
            style={[
              Typeface.footnote,
              { color: Palette.muted, textAlign: 'center' },
            ]}
          >
            Anyone who has these notes can spend them, so share them only with
            the person you're paying. If no one redeems them, you can reclaim
            them back to your balance.
          </Text>
          {state.failures.ecashReclaim && (
            <Text style={[Typeface.footnote, { color: Palette.danger }]}>
              {state.failures.ecashReclaim}
            </Text>
          )}
        </>
      ) : state.failures.ecashSend ? (
        <Text selectable style={[Typeface.callout, { color: Palette.danger }]}>
          {state.failures.ecashSend}
        </Text>
      ) : (
        <View style={{ alignItems: 'center', gap: 8, paddingTop: 80 }}>
          <ActivityIndicator />
          <Text style={[Typeface.callout, { color: Palette.muted }]}>
            Making notes
          </Text>
        </View>
      )}
    </Screen>
  )
}

function EcashStatus({ state }: { state?: EcashSendState }) {
  switch (state) {
    case undefined:
    case EcashSendState.Created:
      return (
        <StatusPill
          text="Waiting for the recipient to redeem"
          tone="warning"
          center
        />
      )
    case EcashSendState.CancelRequested:
      return <StatusPill text="Reclaiming" tone="warning" center />
    case EcashSendState.Canceled:
      return (
        <StatusPill
          text="Reclaimed · back in your balance"
          tone="success"
          center
        />
      )
    case EcashSendState.Redeemed:
      return (
        <StatusPill text="Redeemed by the recipient" tone="success" center />
      )
  }
}
