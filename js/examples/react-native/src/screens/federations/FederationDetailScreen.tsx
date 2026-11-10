import React, { useState } from 'react'
import { Share, Text, View } from 'react-native'
import { useFocusEffect } from '@react-navigation/native'
import { FederationStatus_Tags, type Diagnostic } from '@fedimint/react-native'
import { model, useDemo } from '../../model/DemoModel'
import { codeName } from '../../model/errors'
import { Button } from '../../components/Buttons'
import { Caption, FailureText, StatusPill } from '../../components/Labels'
import { Card, RowDivider, Screen } from '../../components/Surfaces'
import { ReviewRow } from '../../components/Rows'
import { FederationAvatar, networkName } from '../../components/Federation'
import { FederationStatusStyle, PaymentRails } from '../../components/Status'
import { CopyIconButton, middleTruncated } from '../../components/Pasteboard'
import { QRCode } from '../../components/QRCode'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

/** 13 Federation detail: what the attached federation is and how it stands. */
export function FederationDetailScreen({
  showRecovery,
}: {
  showRecovery: () => void
}) {
  const state = useDemo()
  const [showingInvite, setShowingInvite] = useState(false)
  const id = state.federationId

  // Only this screen's own Reconnect: not a switch failed on the list.
  useFocusEffect(
    React.useCallback(() => {
      model.clearFailure('switching')
    }, []),
  )

  const rails = PaymentRails.available(state.capabilities).map(
    PaymentRails.title,
  )
  // Only known if this federation's invite was previewed: the SDK reports guardians in a
  // preview, not on a joined federation.
  const guardians =
    state.invitePreview && state.invitePreview.id === id
      ? state.invitePreview.guardians
      : undefined
  const status = state.federationStatus
  const attaching = model.isAttaching
  const sending = model.isSending

  return (
    <Screen>
      <View
        style={{ flexDirection: 'row', alignItems: 'center', gap: Metrics.gap }}
      >
        <FederationAvatar name={state.federationName} />
        <View style={{ gap: 6 }}>
          <Text style={[Typeface.headline, { color: Palette.ink }]}>
            {state.federationName ?? 'Unnamed federation'}
          </Text>
          <StatusPill
            text={FederationStatusStyle.label(status)}
            tone={FederationStatusStyle.tone(status)}
          />
        </View>
      </View>

      {status?.tag === FederationStatus_Tags.Quarantined && (
        <DiagnosticCard
          diagnostic={(status.inner as { diagnostic: Diagnostic }).diagnostic}
        />
      )}

      {FederationStatusStyle.reconnects(status) && id && (
        <>
          <Button
            title="Reconnect"
            busy={state.working.has('switching') && state.switchTarget === id}
            disabled={attaching || sending}
            onPress={() => model.switchFederation(id)}
          />
          {state.switchTarget === id && (
            <FailureText text={state.failures.switching} />
          )}
        </>
      )}

      {status?.tag === FederationStatus_Tags.Recovering && (
        <Button
          title="View recovery progress"
          kind="secondary"
          onPress={showRecovery}
        />
      )}

      <Card padding={0}>
        <ReviewRow
          label="Network"
          value={networkName(state.federationNetwork)}
        />
        <RowDivider />
        <ReviewRow
          label="Rails"
          value={rails.length ? rails.join(' · ') : 'None'}
        />
        {guardians !== undefined && (
          <>
            <RowDivider />
            <ReviewRow label="Guardians" value={String(guardians)} monospaced />
          </>
        )}
        {id && (
          <>
            <RowDivider />
            <ReviewRow
              label="Federation ID"
              value={middleTruncated(id, 6)}
              monospaced
              trailing={<CopyIconButton value={id} />}
            />
          </>
        )}
      </Card>

      {/* The code that lets someone else join. It admits whoever holds it, so it is shown only
          when asked for. */}
      {state.federationInvite &&
        (showingInvite ? (
          <Card padding={20} style={{ alignItems: 'center', gap: Metrics.gap }}>
            <Caption text="Invite code" />
            <QRCode text={state.federationInvite} size={220} />
            <View
              style={{ flexDirection: 'row', alignItems: 'center', gap: 4 }}
            >
              <Text
                style={[Typeface.mono('Regular', 13), { color: Palette.muted }]}
              >
                {middleTruncated(state.federationInvite, 12)}
              </Text>
              <CopyIconButton value={state.federationInvite} />
            </View>
            <Button
              title="Share invite code"
              icon="square.and.arrow.up"
              onPress={() => Share.share({ message: state.federationInvite! })}
            />
          </Card>
        ) : (
          <Button
            title="Show invite code"
            kind="secondary"
            onPress={() => setShowingInvite(true)}
          />
        ))}
    </Screen>
  )
}

/**
 * The SDK's reason for refusing to run the federation. The message is for people and may change
 * between versions; the code is what is stable.
 */
function DiagnosticCard({ diagnostic }: { diagnostic: Diagnostic }) {
  return (
    <Card style={{ gap: 8 }}>
      <Caption text="Diagnostic" color={Palette.danger} />
      <Text style={[Typeface.figure, { color: Palette.ink }]}>
        {codeName(diagnostic.code)}
      </Text>
      <Text selectable style={[Typeface.footnote, { color: Palette.muted }]}>
        {diagnostic.message}
      </Text>
    </Card>
  )
}
