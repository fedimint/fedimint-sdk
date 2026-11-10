import React from 'react'
import {
  ActivityIndicator,
  Pressable,
  StyleSheet,
  Text,
  View,
  type ViewStyle,
} from 'react-native'
import { Icon, type IconName } from './Icon'
import { Metrics, Palette, Typeface } from '../theme/tokens'

type Kind =
  /** The one action a screen exists for. */
  | 'primary'
  /** The alternative beside or below it. */
  | 'secondary'
  /** An action that gives something up: reclaiming notes, cancelling. */
  | 'destructive'
  /** `primary` and `secondary` for the welcome screen's dark ground. */
  | 'primaryOnDark'
  | 'secondaryOnDark'

const foreground: Record<Kind, string> = {
  primary: '#FFFFFF',
  secondary: Palette.ink,
  destructive: Palette.danger,
  primaryOnDark: Palette.navy,
  secondaryOnDark: '#FFFFFF',
}
const background: Record<Kind, string> = {
  primary: Palette.primary,
  secondary: Palette.surface,
  destructive: Palette.dangerTint,
  primaryOnDark: Palette.surface,
  secondaryOnDark: 'transparent',
}
const border: Partial<Record<Kind, string>> = {
  secondary: Palette.border,
  secondaryOnDark: 'rgba(255,255,255,0.35)',
}

/**
 * The design's full-width button. `busy` puts a spinner in front of the title, for an action
 * whose second tap would be a second real spend; it does not disable the button by itself.
 */
export function Button({
  title,
  onPress,
  kind = 'primary',
  disabled = false,
  busy = false,
  icon,
  style,
}: {
  title: string
  onPress: () => void
  kind?: Kind
  disabled?: boolean
  busy?: boolean
  /** Shown before the title, as the iOS demo's `Label` with a system image. */
  icon?: IconName
  style?: ViewStyle
}) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityState={{ disabled, busy }}
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => [
        styles.button,
        {
          backgroundColor: background[kind],
          borderColor: border[kind] ?? 'transparent',
          borderWidth: border[kind] ? Metrics.borderWidth : 0,
          opacity: disabled ? 0.4 : pressed ? 0.8 : 1,
        },
        style,
      ]}
    >
      <View style={styles.label}>
        {busy && <ActivityIndicator color={foreground[kind]} />}
        {icon && !busy && <Icon name={icon} color={foreground[kind]} />}
        <Text style={[Typeface.button, { color: foreground[kind] }]}>
          {title}
        </Text>
      </View>
    </Pressable>
  )
}

/** A plain text action in the primary colour: "Back up later". */
export function TextButton({
  title,
  onPress,
}: {
  title: string
  onPress: () => void
}) {
  return (
    <Pressable accessibilityRole="button" onPress={onPress} style={styles.text}>
      {({ pressed }) => (
        <Text
          style={[
            Typeface.button,
            { color: Palette.primary, opacity: pressed ? 0.6 : 1 },
          ]}
        >
          {title}
        </Text>
      )}
    </Pressable>
  )
}

const styles = StyleSheet.create({
  text: { alignSelf: 'center', paddingVertical: 6 },
  button: {
    minHeight: Metrics.buttonHeight,
    borderRadius: Metrics.controlRadius,
    paddingHorizontal: Metrics.gap,
    alignItems: 'center',
    justifyContent: 'center',
    alignSelf: 'stretch',
  },
  label: { flexDirection: 'row', alignItems: 'center', gap: 8 },
})
