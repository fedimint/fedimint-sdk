import React from 'react'
import {
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
  type TextInputProps,
} from 'react-native'
import { Metrics, Palette, Typeface } from '../theme/tokens'

/** A label above an input, the way every form in the design is laid out. */
export function Field({
  label,
  children,
}: {
  label: string
  children: React.ReactNode
}) {
  return (
    <View style={{ gap: 8 }}>
      <Text style={[Typeface.geist('Medium', 14), { color: Palette.muted }]}>
        {label}
      </Text>
      {children}
    </View>
  )
}

/** The design's input box. */
export function Input({
  monospaced = false,
  style,
  ...props
}: TextInputProps & { monospaced?: boolean }) {
  return (
    <TextInput
      placeholderTextColor={Palette.muted}
      autoCorrect={false}
      autoCapitalize="none"
      style={[styles.input, monospaced ? Typeface.code : Typeface.body, style]}
      {...props}
    />
  )
}

/**
 * The Lightning / Ecash / Onchain control at the top of Send and Receive. Callers pass only the
 * rails the attached federation supports: the design hides a rail it cannot use.
 */
export function RailPicker<Rail extends string>({
  rails,
  selection,
  onSelect,
  title,
}: {
  rails: Rail[]
  selection: Rail
  onSelect: (rail: Rail) => void
  title: (rail: Rail) => string
}) {
  return (
    <View style={styles.picker}>
      {rails.map((rail) => {
        const selected = rail === selection
        return (
          <Pressable
            key={rail}
            accessibilityRole="button"
            accessibilityState={{ selected }}
            onPress={() => onSelect(rail)}
            style={[styles.segment, selected && styles.selected]}
          >
            <Text
              style={[
                Typeface.geist('Medium', 15),
                { color: selected ? Palette.ink : Palette.muted },
              ]}
            >
              {title(rail)}
            </Text>
          </Pressable>
        )
      })}
    </View>
  )
}

const styles = StyleSheet.create({
  input: {
    color: Palette.ink,
    paddingHorizontal: 14,
    paddingVertical: 12,
    minHeight: Metrics.buttonHeight,
    backgroundColor: Palette.surface,
    borderRadius: Metrics.controlRadius,
    borderWidth: Metrics.borderWidth,
    borderColor: Palette.border,
  },
  picker: {
    flexDirection: 'row',
    gap: 4,
    padding: 4,
    backgroundColor: Palette.subtle,
    borderRadius: 13,
  },
  segment: {
    flex: 1,
    minHeight: 40,
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 10,
  },
  selected: {
    backgroundColor: Palette.surface,
    shadowColor: '#000',
    shadowOpacity: 0.06,
    shadowRadius: 2,
    shadowOffset: { width: 0, height: 1 },
    elevation: 1,
  },
})
