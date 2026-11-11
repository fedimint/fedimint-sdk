import React from 'react'
import { StyleSheet, Text, View } from 'react-native'
import { Icon } from '../../components/Icon'
import { model, useDemo } from '../../model/DemoModel'
import { Button, TextButton } from '../../components/Buttons'
import { Caption } from '../../components/Labels'
import { SeedGrid } from '../../components/SeedGrid'
import { Card, Screen } from '../../components/Surfaces'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

/**
 * 02 Back up: the new wallet's recovery phrase, numbered, behind the same device-owner
 * authentication `toggleSeed()` imposes everywhere else.
 */
export function BackupScreen({
  next,
  later,
  step = 'Step 1 of 2',
}: {
  next: () => void
  later: () => void
  /** "Step 1 of 2" during onboarding; `null` when opened from the wallet later. */
  step?: string | null
}) {
  const state = useDemo()
  const words = state.seedWords

  return (
    <Screen
      gap={Metrics.gap}
      footer={
        <>
          <Button
            title="I've written it down"
            disabled={words.length === 0}
            onPress={next}
          />
          {/* A device without a passcode can never show the phrase (`toggleSeed` fails
              closed), so this screen needs a way out. The phrase stays reachable later. */}
          {words.length === 0 && (
            <TextButton title="Back up later" onPress={later} />
          )}
        </>
      }
    >
      {step && <Caption text={step} />}
      <Text style={[Typeface.title, { color: Palette.ink }]}>
        Write down your recovery phrase
      </Text>
      <Text style={[Typeface.callout, { color: Palette.muted }]}>
        These words are the only way to restore your wallet. Write them on
        paper, in order.
      </Text>
      <View style={{ marginTop: Metrics.gap }}>
        {words.length === 0 ? (
          <Card padding={24} style={styles.reveal}>
            <Icon name="lock" size={28} color={Palette.muted} />
            <Text style={[Typeface.callout, { color: Palette.muted }]}>
              Confirm it's you to see the words.
            </Text>
            <Button
              title="Show recovery phrase"
              kind="secondary"
              onPress={() => model.toggleSeed()}
            />
            {state.seedFailure && (
              <Text style={[Typeface.footnote, styles.failure]}>
                {state.seedFailure}
              </Text>
            )}
          </Card>
        ) : (
          <SeedGrid
            count={words.length}
            cell={(index) => (
              <Text
                selectable={false}
                style={[Typeface.geist('Medium', 16), { color: Palette.ink }]}
              >
                {words[index]}
              </Text>
            )}
          />
        )}
      </View>
      <View style={styles.notice}>
        <Icon
          name="checkmark.shield"
          size={13}
          weight="regular"
          color={Palette.muted}
        />
        <Text style={[Typeface.footnote, { color: Palette.muted, flex: 1 }]}>
          Never share these words. No one from Fedimint will ever ask for them.
        </Text>
      </View>
    </Screen>
  )
}

const styles = StyleSheet.create({
  reveal: { alignItems: 'center', gap: Metrics.gap },
  failure: { color: Palette.danger, textAlign: 'center' },
  notice: { flexDirection: 'row', gap: 8, alignItems: 'center', marginTop: 4 },
})
