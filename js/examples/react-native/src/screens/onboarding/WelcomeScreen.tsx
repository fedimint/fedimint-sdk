import React from 'react'
import { StyleSheet, Text, View } from 'react-native'
import { useSafeAreaInsets } from 'react-native-safe-area-context'
import { model, useDemo } from '../../model/DemoModel'
import { Button } from '../../components/Buttons'
import { Caption } from '../../components/Labels'
import { FedimintLogoOnDark, FedimintMark } from '../../components/Brand'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

/**
 * 01 Welcome: no wallet in storage yet, so the only two things that can happen are creating one
 * or restoring one from a recovery phrase.
 */
export function WelcomeScreen({ restore }: { restore: () => void }) {
  const state = useDemo()
  const attaching = model.isAttaching
  // An open that failed leaves the reason here; one that worked replaces this screen.
  const failure = !attaching ? state.failures.wallet : undefined
  // The insets as padding, not the library's SafeAreaView: on Android that view's layout trips
  // an assertion in React Native 0.83's debug build and the app aborts at launch.
  const insets = useSafeAreaInsets()

  return (
    <View style={styles.ground}>
      {/* Bleeds off the trailing edge, the way the design crops it. */}
      <View
        style={styles.mark}
        accessibilityElementsHidden
        importantForAccessibility="no-hide-descendants"
      >
        <FedimintMark width={280} />
      </View>
      <View
        style={[
          styles.content,
          {
            paddingTop: insets.top + 16,
            paddingBottom: insets.bottom + 16,
            paddingLeft: insets.left + Metrics.screenPadding,
            paddingRight: insets.right + Metrics.screenPadding,
          },
        ]}
      >
        <FedimintLogoOnDark height={26} />
        <View style={{ flex: 1, minHeight: Metrics.sectionGap }} />
        <Caption text="Fedimint SDK · Demo wallet" color={Palette.accent} />
        <Text style={[Typeface.title, styles.title]}>
          Bitcoin custody, shared by people you trust.
        </Text>
        <Text style={[Typeface.callout, styles.subtitle]}>
          A reference wallet showing every SDK flow, identical on iOS, Android
          and the web.
        </Text>
        <View style={styles.buttons}>
          <Button
            title="Create new wallet"
            kind="primaryOnDark"
            busy={attaching}
            disabled={attaching}
            onPress={() => {
              // Explicit rather than assumed: the field is shared with Restore, and a wallet
              // created here must never pick up words left behind by an abandoned restore.
              model.setRestoreWords('')
              model.openWallet()
            }}
          />
          <Button
            title="Restore from recovery phrase"
            kind="secondaryOnDark"
            disabled={attaching}
            onPress={restore}
          />
        </View>
        {failure && (
          <Text style={[Typeface.footnote, styles.failure]}>{failure}</Text>
        )}
        <Text style={[Typeface.mono('Regular', 12), styles.testFunds]}>
          Use test funds only
        </Text>
      </View>
    </View>
  )
}

const styles = StyleSheet.create({
  ground: { flex: 1, backgroundColor: Palette.navy },
  mark: { position: 'absolute', right: -48, top: 150 },
  content: { flex: 1 },
  title: { color: '#FFFFFF', marginTop: 10 },
  subtitle: { color: 'rgba(255,255,255,0.65)', marginTop: Metrics.gap },
  buttons: { gap: Metrics.gap, marginTop: 32 },
  failure: { color: Palette.dangerTint, marginTop: Metrics.gap },
  testFunds: {
    color: 'rgba(255,255,255,0.5)',
    textAlign: 'center',
    marginTop: Metrics.sectionGap,
  },
})
