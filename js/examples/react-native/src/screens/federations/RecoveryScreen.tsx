import React from 'react'
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native'
import { useFocusEffect } from '@react-navigation/native'
import {
  RecoveryState_Tags,
  type RecoveryProgress,
} from '@fedimint/react-native'
import { model, useCloseOnAttach, useDemo } from '../../model/DemoModel'
import { Button } from '../../components/Buttons'
import { Caption, FailureText, StatusPill } from '../../components/Labels'
import { Card, Screen } from '../../components/Surfaces'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

/**
 * 10 Recovery: a restored wallet rescanning the federation's history for what its seed owns.
 * Until the rescan is done the federation is locked: sends and receives are refused, because
 * spending from notes the rescan has not reached yet could spend one twice. A stopped attempt
 * does not unlock it; only `done` does.
 */
export function RecoveryScreen({ close }: { close: () => void }) {
  const state = useDemo()
  const recovery = state.recoveryState

  // On every appearance, as iOS's `onAppear`: coming back to a watcher that stopped resumes it.
  useFocusEffect(
    React.useCallback(() => {
      model.followRecovery()
    }, []),
  )

  useCloseOnAttach(close)

  const progress =
    recovery?.tag === RecoveryState_Tags.Running
      ? (recovery.inner as { progress?: RecoveryProgress }).progress
      : undefined
  const fraction =
    progress && progress.total > 0
      ? progress.complete / progress.total
      : undefined
  const headline =
    recovery?.tag === RecoveryState_Tags.Done
      ? '100%'
      : recovery?.tag === RecoveryState_Tags.Running && fraction !== undefined
        ? `${Math.floor(fraction * 100)}%`
        : '…'
  const title =
    recovery?.tag === RecoveryState_Tags.Done
      ? 'Your funds are recovered'
      : recovery?.tag === RecoveryState_Tags.Failed
        ? 'Recovery stopped'
        : 'Recovering your funds'

  return (
    <Screen
      gap={Metrics.gap}
      footer={
        recovery?.tag === RecoveryState_Tags.Failed ? (
          <Button
            title="Resume recovery"
            disabled={state.working.has('recovery')}
            onPress={() => model.resumeRecovery()}
          />
        ) : undefined
      }
    >
      <Caption text="Restoring wallet" />
      <Text style={[Typeface.amount, { color: Palette.ink }]}>{headline}</Text>
      <Text style={[Typeface.title, { color: Palette.ink }]}>{title}</Text>
      <Text style={[Typeface.callout, { color: Palette.muted }]}>
        Scanning the federation's history for ecash issued to your recovery
        phrase. This can take a few minutes, and keeps running if you leave this
        screen.
      </Text>
      <Card style={{ gap: 10, marginTop: Metrics.gap }}>
        {recovery?.tag === RecoveryState_Tags.Running &&
        progress &&
        progress.total > 0 ? (
          <>
            <View style={styles.track}>
              <View
                style={[styles.fill, { width: `${(fraction ?? 0) * 100}%` }]}
              />
            </View>
            <Text
              style={[Typeface.mono('Regular', 13), { color: Palette.muted }]}
            >
              {progress.complete} of {progress.total}
            </Text>
          </>
        ) : recovery?.tag === RecoveryState_Tags.Done ? (
          <StatusPill
            text="Done · sends and receives are open again"
            tone="success"
          />
        ) : recovery?.tag === RecoveryState_Tags.Failed ? (
          <>
            <StatusPill text="Stopped" tone="danger" />
            <Text style={[Typeface.footnote, { color: Palette.muted }]}>
              {(recovery.inner as { reason: string }).reason}
            </Text>
          </>
        ) : (
          // The rescan has not said how much work it has yet.
          <ActivityIndicator />
        )}
      </Card>
      {state.watchFailures.recovery && (
        <Text
          selectable
          style={[Typeface.footnote, { color: Palette.warning }]}
        >
          Stopped following the recovery: {state.watchFailures.recovery}. Open
          this screen again to follow it.
        </Text>
      )}
      <FailureText text={state.failures.recovery} />
    </Screen>
  )
}

const styles = StyleSheet.create({
  track: {
    height: 4,
    borderRadius: 2,
    backgroundColor: Palette.subtle,
    overflow: 'hidden',
  },
  fill: { height: 4, backgroundColor: Palette.primary },
})
