import React, { useEffect, useRef } from 'react'
import { Text, View } from 'react-native'
import Clipboard from '@react-native-clipboard/clipboard'
import type { FederationPreview } from '@fedimint/react-native'
import { model, useDemo } from '../../model/DemoModel'
import { Button } from '../../components/Buttons'
import { FailureText } from '../../components/Labels'
import { Card, RowDivider, Screen } from '../../components/Surfaces'
import { Field, Input } from '../../components/Inputs'
import { ReviewRow } from '../../components/Rows'
import { networkName } from '../../components/Federation'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

/**
 * 12 Join: an invite code, a preview of the federation it names, then join.
 *
 * A wallet restored from a recovery phrase may already hold funds in the federation it joins,
 * which a plain join would not look for. So unless the wallet is known to have been created on
 * this device, the action is the SDK's `recover`, which joins and rescans.
 */
export function JoinScreen({ close }: { close: () => void }) {
  const state = useDemo()
  const recovering = state.recoversOnJoin
  const working = state.working.has('join')
  const sending = model.isSending
  const attaching = model.isAttaching
  const empty = state.invite.trim().length === 0
  /**
   * Set by Join, so the screen can tell its own join finishing apart from a preview finishing:
   * both report on the same line.
   */
  const joining = useRef(false)

  // A join that finished without failing is done here, including one that found the federation
  // already joined and reattached to it. Joining the one attached and running fails instead, so
  // its message stays on screen.
  useEffect(() => {
    if (!joining.current || working) return
    joining.current = false
    if (model.getState().failures.join === undefined) close()
  }, [working, close])

  return (
    <Screen
      gap={Metrics.gap}
      footer={
        <Button
          title={recovering ? 'Join and recover' : 'Join'}
          busy={working}
          disabled={empty || working || sending || attaching}
          onPress={() => {
            // Only for a join that started: one refused in the frame the button lags would
            // otherwise leave this set, and the next Preview to finish would close the screen.
            if (recovering ? model.recover() : model.join())
              joining.current = true
          }}
        />
      }
    >
      <Text style={[Typeface.title, { color: Palette.ink }]}>
        Join a federation
      </Text>
      <Text style={[Typeface.callout, { color: Palette.muted }]}>
        Paste an invite code from one of its guardians.
      </Text>
      <View style={{ marginTop: Metrics.gap }}>
        <Field label="Invite code">
          <Input
            monospaced
            multiline
            value={state.invite}
            onChangeText={(text) => model.setInvite(text)}
            style={{ minHeight: 110, textAlignVertical: 'top' }}
          />
        </Field>
      </View>
      <View style={{ flexDirection: 'row', gap: Metrics.gap }}>
        <Button
          title="Paste"
          icon="doc.on.clipboard"
          kind="secondary"
          style={{ flex: 1 }}
          onPress={() =>
            Clipboard.getString().then(
              (text) => text && model.setInvite(text.trim()),
            )
          }
        />
        <Button
          title="Preview"
          kind="secondary"
          style={{ flex: 1 }}
          disabled={empty || working}
          onPress={() => model.preview()}
        />
      </View>
      {state.invitePreview && (
        <View style={{ marginTop: Metrics.gap }}>
          <PreviewCard preview={state.invitePreview} />
        </View>
      )}
      {recovering && (
        <Text
          style={[Typeface.footnote, { color: Palette.muted, marginTop: 4 }]}
        >
          Joining also rescans the federation for funds this wallet's recovery
          phrase may already hold there.
        </Text>
      )}
      <FailureText text={state.failures.join} />
    </Screen>
  )
}

/** What a federation is, read from its invite code before joining it. */
function PreviewCard({ preview }: { preview: FederationPreview }) {
  return (
    <Card padding={0}>
      <ReviewRow label="Name" value={preview.name ?? 'Unnamed'} />
      <RowDivider />
      <ReviewRow label="Network" value={networkName(preview.network)} />
      <RowDivider />
      <ReviewRow
        label="Guardians"
        value={String(preview.guardians)}
        monospaced
      />
      <RowDivider />
      <ReviewRow label="Modules" value={preview.modules.join(', ')} />
    </Card>
  )
}
