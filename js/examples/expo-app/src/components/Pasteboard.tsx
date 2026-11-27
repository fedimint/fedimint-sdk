import { useEffect, useRef, useState } from 'react'
import { Pressable } from 'react-native'
import * as Clipboard from 'expo-clipboard'
import { FedimintDemo } from '../../modules/fedimint-demo'
import { Icon } from './Icon'
import { Palette } from '../theme/tokens'

/** How long bearer value stays on the clipboard, in seconds. */
const bearerLifetime = 120

/**
 * Copies `value` to the clipboard. A `bearer` value is money to whoever reads it (ecash notes),
 * so it goes through the app's native module, which keeps it off the user's other devices and
 * clears it after two minutes. Invoices and addresses keep the plain copy: pasting them on
 * another device is the point.
 */
async function copy(value: string, bearer = false): Promise<void> {
  if (bearer) await FedimintDemo.copyBearer(value, bearerLifetime)
  else await Clipboard.setStringAsync(value)
}

/** The copy glyph beside an invoice, address or note bundle, confirming the copy for a moment. */
export function CopyIconButton({
  value,
  bearer = false,
}: {
  value: string
  bearer?: boolean
}) {
  /** What the last tap came to, shown for a moment. */
  const [outcome, setOutcome] = useState<'copied' | 'failed'>()
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined)
  const mounted = useRef(true)
  useEffect(
    () => () => {
      mounted.current = false
      if (timer.current) clearTimeout(timer.current)
    },
    [],
  )
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={
        outcome === 'copied'
          ? 'Copied'
          : outcome === 'failed'
            ? 'Copy failed'
            : 'Copy'
      }
      hitSlop={6}
      style={{
        width: 32,
        height: 32,
        alignItems: 'center',
        justifyContent: 'center',
      }}
      onPress={() => {
        // The checkmark only for a copy that happened, and a warning for one that did not: the
        // clipboard then still holds whatever was copied before, which must not pass for this.
        const show = (result: 'copied' | 'failed') => {
          if (!mounted.current) return
          setOutcome(result)
          if (timer.current) clearTimeout(timer.current)
          timer.current = setTimeout(() => setOutcome(undefined), 2000)
        }
        copy(value, bearer).then(
          () => show('copied'),
          () => show('failed'),
        )
      }}
    >
      <Icon
        name={
          outcome === 'copied'
            ? 'checkmark'
            : outcome === 'failed'
              ? 'exclamationmark.triangle'
              : 'doc.on.doc'
        }
        color={
          outcome === 'copied'
            ? Palette.success
            : outcome === 'failed'
              ? Palette.danger
              : Palette.ink
        }
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
