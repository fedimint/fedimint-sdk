import type { ReactNode } from 'react'
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
  children: ReactNode
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

/** iOS's `placeholderText` colour in the light scheme. */
export const placeholder = 'rgba(60, 60, 67, 0.3)'

const inputPaddingVertical = 12
const inputPaddingHorizontal = 14
/** A multiline box's text inset, on top of its padding. */
const editorInset = { top: 8, bottom: 4, left: 5 }

/** The design's input box. */
export function Input({
  monospaced = false,
  textHeight,
  style,
  ...props
}: TextInputProps & {
  monospaced?: boolean
  /** A multiline box's minimum text height, padding excluded. */
  textHeight?: number
}) {
  return (
    <TextInput
      placeholderTextColor={placeholder}
      autoCorrect={false}
      autoCapitalize="none"
      style={[
        styles.input,
        monospaced ? Typeface.code : Typeface.body,
        textHeight !== undefined && {
          minHeight: textHeight + 2 * inputPaddingVertical,
          paddingTop: inputPaddingVertical + editorInset.top,
          paddingBottom: inputPaddingVertical + editorInset.bottom,
          paddingLeft: inputPaddingHorizontal + editorInset.left,
          textAlignVertical: 'top',
        },
        style,
      ]}
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
    paddingHorizontal: inputPaddingHorizontal,
    paddingVertical: inputPaddingVertical,
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
