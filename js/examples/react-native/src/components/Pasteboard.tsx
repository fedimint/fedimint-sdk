import React, { useEffect, useRef, useState } from 'react'
import { NativeModules, Pressable } from 'react-native'
import Clipboard from '@react-native-clipboard/clipboard'
import { Icon } from './Icon'
import { Palette } from '../theme/tokens'

/** How long bearer value stays on the clipboard, in seconds. */
const bearerLifetime = 120

/** The example's own native module: ios/ReactNativeExample/BearerPasteboard.m and its Android twin. */
const BearerPasteboard = NativeModules.BearerPasteboard as {
  copy(value: string, lifetimeSeconds: number): void
}

/**
 * Copies `value` to the clipboard. A `bearer` value is money to whoever reads it: ecash notes,
 * not an invoice or an address, which are useless to anyone but the payer. Those stay on this
 * device, so Universal Clipboard never syncs them to the user's other devices, and expire after
 * two minutes, as in the iOS demo. The other values keep the plain copy, because pasting an
 * invoice or address on another device is the point.
 */
export function copy(value: string, bearer = false) {
  if (bearer) BearerPasteboard.copy(value, bearerLifetime)
  else Clipboard.setString(value)
}

/** The copy glyph beside an invoice, address or note bundle, confirming the copy for a moment. */
export function CopyIconButton({
  value,
  bearer = false,
}: {
  value: string
  bearer?: boolean
}) {
  const [copied, setCopied] = useState(false)
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined)
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current)
    },
    [],
  )
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={copied ? 'Copied' : 'Copy'}
      hitSlop={6}
      style={{
        width: 32,
        height: 32,
        alignItems: 'center',
        justifyContent: 'center',
      }}
      onPress={() => {
        copy(value, bearer)
        setCopied(true)
        if (timer.current) clearTimeout(timer.current)
        timer.current = setTimeout(() => setCopied(false), 2000)
      }}
    >
      <Icon
        name={copied ? 'checkmark' : 'doc.on.doc'}
        color={copied ? Palette.success : Palette.ink}
      />
    </Pressable>
  )
}

/** "lnbc1…xyz": the start and end of a long value, for a line that shows it whole elsewhere. */
export function middleTruncated(value: string, keep = 14): string {
  return value.length <= keep * 2
    ? value
    : `${value.slice(0, keep)}…${value.slice(-keep)}`
}
