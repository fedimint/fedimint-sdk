import React, { useState } from 'react'
import { Pressable, StyleSheet, Text, View } from 'react-native'
import { Button } from '../../components/Buttons'
import { Caption } from '../../components/Labels'
import { Screen } from '../../components/Surfaces'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

/** How many positions are checked. */
const challengeCount = 3

interface Challenge {
  /** Zero-based index into the phrase. */
  position: number
  options: string[]
}

function shuffled<T>(items: T[]): T[] {
  const copy = [...items]
  for (let i = copy.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1))
    ;[copy[i], copy[j]] = [copy[j]!, copy[i]!]
  }
  return copy
}

/**
 * Three positions in phrase order, each offered with two other words from the same phrase.
 * Decoys come from the phrase itself so that none of them can be ruled out as never shown.
 */
function makeChallenges(words: string[]): Challenge[] {
  const positions = shuffled(words.map((_, i) => i))
    .slice(0, challengeCount)
    .sort((a, b) => a - b)
  return positions.map((position) => {
    const answer = words[position]!
    // Distinct from the answer by spelling, not by position: a phrase can repeat a word, and
    // two identical chips would be unanswerable.
    const decoys = shuffled(
      [...new Set(words)].filter((word) => word !== answer),
    ).slice(0, 2)
    return { position, options: shuffled([answer, ...decoys]) }
  })
}

/**
 * 03 Verify: asks for three of the phrase's words back, each picked from three choices, before
 * the wallet opens. Entirely local: nothing in the SDK records that a phrase was backed up, so
 * this proves only that the words were read, which is all a demo can claim.
 */
export function VerifyScreen({
  words,
  finish,
}: {
  words: string[]
  finish: () => void
}) {
  const [challenges] = useState(() => makeChallenges(words))
  const [picks, setPicks] = useState<Record<number, string>>({})
  const confirmed = challenges.filter(
    (c) => picks[c.position] === words[c.position],
  ).length

  return (
    <Screen
      gap={Metrics.gap}
      footer={
        <>
          <Text style={[Typeface.mono('Regular', 13), styles.count]}>
            {confirmed} / {challenges.length} confirmed
          </Text>
          <Button
            title="Continue"
            disabled={confirmed < challenges.length}
            onPress={finish}
          />
        </>
      }
    >
      <Caption text="Step 2 of 2" />
      <Text style={[Typeface.title, { color: Palette.ink }]}>
        Confirm three words
      </Text>
      <Text style={[Typeface.callout, { color: Palette.muted }]}>
        Pick the right word for each position.
      </Text>
      {challenges.map((challenge) => (
        <View key={challenge.position} style={styles.challenge}>
          <Caption
            text={`Word ${String(challenge.position + 1).padStart(2, '0')}`}
          />
          <View style={styles.options}>
            {challenge.options.map((option) => {
              const picked = picks[challenge.position] === option
              const right = option === words[challenge.position]
              return (
                <Pressable
                  key={option}
                  accessibilityRole="button"
                  accessibilityState={{ selected: picked }}
                  onPress={() =>
                    setPicks({ ...picks, [challenge.position]: option })
                  }
                  style={[
                    styles.chip,
                    picked
                      ? {
                          backgroundColor: right
                            ? Palette.navy
                            : Palette.dangerTint,
                          borderColor: 'transparent',
                        }
                      : null,
                  ]}
                >
                  <Text
                    style={[
                      Typeface.geist('Medium', 16),
                      {
                        color: picked
                          ? right
                            ? '#FFFFFF'
                            : Palette.danger
                          : Palette.ink,
                      },
                    ]}
                  >
                    {option}
                  </Text>
                </Pressable>
              )
            })}
          </View>
        </View>
      ))}
    </Screen>
  )
}

const styles = StyleSheet.create({
  count: { color: Palette.muted, textAlign: 'center' },
  challenge: { gap: 10, marginTop: Metrics.sectionGap },
  options: { flexDirection: 'row', gap: 10 },
  chip: {
    flex: 1,
    minHeight: Metrics.buttonHeight,
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: Metrics.controlRadius,
    borderWidth: Metrics.borderWidth,
    borderColor: Palette.border,
    backgroundColor: Palette.surface,
  },
})
