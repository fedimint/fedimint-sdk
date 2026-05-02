import React from 'react'
import { View, Text, TouchableOpacity } from 'react-native'
import s from './styles'

export const SectionCard: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => <View style={s.section}>{children}</View>

export const SectionTitle: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => <Text style={s.sectionTitle}>{String(children)}</Text>

export const Btn: React.FC<{
  title: string
  onPress: () => void
  disabled?: boolean
  active?: boolean
  small?: boolean
  primary?: boolean
}> = ({ title, onPress, disabled, active, small, primary }) => (
  <TouchableOpacity
    onPress={onPress}
    disabled={disabled}
    style={[
      s.btn,
      active && s.btnActive,
      small && s.btnSmall,
      primary && s.btnPrimary,
      disabled && s.btnDisabled,
    ]}
  >
    <Text
      style={[
        s.btnText,
        small && s.btnTextSmall,
        disabled && s.btnTextDisabled,
      ]}
    >
      {title}
    </Text>
  </TouchableOpacity>
)

export const SuccessBox: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => (
  <View style={s.success}>
    <Text style={s.successText}>{children}</Text>
  </View>
)

export const ErrorBox: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => (
  <View style={s.error}>
    <Text style={s.errorText}>{children}</Text>
  </View>
)

export const Row: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <View style={s.row}>{children}</View>
)
