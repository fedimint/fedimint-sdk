import { useEffect, useRef, useState } from 'react'
import {
  Alert,
  Platform,
  Pressable,
  StatusBar,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native'
import { model, useDemo } from '../../model/DemoModel'
import { Button } from '../../components/Buttons'
import { Caption, FailureText } from '../../components/Labels'
import { SeedGrid } from '../../components/SeedGrid'
import { Screen } from '../../components/Surfaces'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

/**
 * The phrase lengths BIP-39 defines, all of which the SDK accepts. The picker offers the two in
 * common use; a pasted phrase of any of these lengths resizes the grid to fit it. The SDK still
 * does the real validation, checksum included.
 */
const validWordCounts = new Set([12, 15, 18, 21, 24])

/**
 * 11 Restore: takes a recovery phrase and opens the wallet it belongs to. With `replaced`, the
 * same screen from the wallet's menu: it replaces the open wallet instead, after a confirmation,
 * and calls `replaced` once the new one is open.
 */
export function RestoreScreen({ replaced }: { replaced?: () => void }) {
  const state = useDemo()
  const replacing = replaced !== undefined
  // A failure is about one visit, as onboarding clears Restore's on the way out.
  useEffect(() => {
    if (!replacing) return
    model.clearFailure('replace')
    return () => model.clearFailure('replace')
  }, [replacing])
  const attaching = model.isAttaching
  const [words, setWords] = useState<string[]>(() => Array(12).fill(''))
  const [focused, setFocused] = useState<number>()
  const inputs = useRef<(TextInput | null)[]>([])
  const complete = words.every((word) => word.length > 0)

  const update = (next: string[]) => {
    setWords(next)
    // Replacing hands its words over on the press: the open wallet's model keeps no copy.
    if (!replacing) model.setRestoreWords(next.join(' '))
  }

  /** Keeps the words already typed, dropping only those past the new length. */
  const resized = (current: string[], count: number) =>
    [...current, ...Array(count).fill('')].slice(0, count) as string[]

  /**
   * One cell's text. A space or a paste moves on: whatever follows the first word spills into
   * the next cells, so pasting the whole phrase into any cell fills the grid from there.
   */
  const type = (index: number, typed: string) => {
    const parts = typed.toLowerCase().split(/\s+/).filter(Boolean)
    if (parts.length <= 1 && !/\s$/.test(typed)) {
      const next = [...words]
      next[index] = typed.trim().toLowerCase()
      update(next)
      return
    }
    let next = [...words]
    // A whole phrase pasted into the first cell sets the length too.
    if (
      index === 0 &&
      parts.length !== next.length &&
      validWordCounts.has(parts.length)
    ) {
      next = resized(next, parts.length)
    }
    parts.forEach((part, offset) => {
      if (index + offset < next.length) next[index + offset] = part
    })
    update(next)
    const following = index + Math.max(parts.length, 1)
    if (following < next.length) inputs.current[following]?.focus()
    else inputs.current[index]?.blur()
  }

  return (
    <Screen
      gap={Metrics.gap}
      footer={
        <Button
          title={replacing ? 'Replace wallet' : 'Restore wallet'}
          busy={replacing ? state.replacing : attaching}
          disabled={!complete || attaching || state.replacing}
          onPress={() => {
            inputs.current.forEach((input) => input?.blur())
            if (!replacing) {
              model.openWallet()
              return
            }
            Alert.alert(
              'Replace this wallet?',
              'This deletes the wallet open now from this device. Getting back to it takes ' +
                'its recovery phrase, so if it holds funds, make sure you have its phrase ' +
                'written down first.',
              [
                { text: 'Cancel', style: 'cancel' },
                {
                  text: 'Replace',
                  style: 'destructive',
                  onPress: () => {
                    model.replaceWallet(words).then((done) => {
                      if (done) replaced()
                    })
                  },
                },
              ],
            )
          }}
        />
      }
    >
      {/* Restore is cream over a navy Welcome: dark status-bar text while it is up. Unmounting
          restores the one beneath. */}
      {!replacing && <StatusBar barStyle="dark-content" />}
      <Caption text={replacing ? 'Replace' : 'Restore'} />
      <Text
        lineBreakStrategyIOS="standard"
        style={[Typeface.title, { color: Palette.ink }]}
      >
        {replacing
          ? 'Enter the phrase to restore instead'
          : 'Enter your recovery phrase'}
      </Text>
      <Text style={[Typeface.callout, { color: Palette.muted }]}>
        {replacing
          ? `Type the ${words.length} words in order. The wallet open now is deleted from this device and this one opens in its place. Join its federations again to recover their funds.`
          : `Type the ${words.length} words in order. Restoring starts recovery for every federation you join.`}
      </Text>
      <View style={styles.picker}>
        {[12, 24].map((count) => {
          const selected = words.length === count
          return (
            <Pressable
              key={count}
              accessibilityRole="button"
              accessibilityState={{ selected }}
              onPress={() => {
                update(resized(words, count))
                // The cell may go with the grid, and an unmounted input sends no blur.
                if (focused !== undefined && focused >= count)
                  setFocused(undefined)
              }}
              style={[
                styles.chip,
                selected ? styles.chipSelected : styles.chipIdle,
              ]}
            >
              <Text
                style={[
                  Typeface.geist('Medium', 15),
                  { color: selected ? '#FFFFFF' : Palette.ink },
                ]}
              >
                {count} words
              </Text>
            </Pressable>
          )
        })}
      </View>
      <SeedGrid
        count={words.length}
        highlighted={focused}
        cell={(index) => (
          <TextInput
            ref={(input) => {
              inputs.current[index] = input
            }}
            value={words[index]}
            onChangeText={(text) => type(index, text)}
            onFocus={() => setFocused(index)}
            onBlur={() =>
              setFocused((current) => (current === index ? undefined : current))
            }
            placeholder="·····"
            placeholderTextColor={Palette.border}
            autoCorrect={false}
            autoCapitalize="none"
            // `ascii-capable` is iOS only; on Android `visible-password` stops keyboards such as
            // Gboard suggesting or learning the seed words.
            keyboardType={
              Platform.OS === 'android' ? 'visible-password' : 'ascii-capable'
            }
            importantForAutofill="no"
            returnKeyType={index === words.length - 1 ? 'done' : 'next'}
            blurOnSubmit={index === words.length - 1}
            onSubmitEditing={() => inputs.current[index + 1]?.focus()}
            accessibilityLabel={`Word ${index + 1}`}
            style={[
              Typeface.geist('Medium', 16),
              styles.word,
              { color: focused === index ? Palette.primary : Palette.ink },
            ]}
          />
        )}
      />
      {/* Wrong checksum, unknown word, or storage that already holds a different seed: the
          SDK's reason, shown where it can be fixed. */}
      {!attaching && (
        <FailureText
          text={replacing ? state.failures.replace : state.failures.wallet}
        />
      )}
    </Screen>
  )
}

const styles = StyleSheet.create({
  picker: { flexDirection: 'row', gap: 8, marginTop: 4 },
  chip: {
    paddingHorizontal: 16,
    height: 36,
    borderRadius: 18,
    justifyContent: 'center',
  },
  chipSelected: { backgroundColor: Palette.navy },
  chipIdle: {
    backgroundColor: Palette.surface,
    borderWidth: 1,
    borderColor: Palette.border,
  },
  word: { paddingVertical: 0 },
})
