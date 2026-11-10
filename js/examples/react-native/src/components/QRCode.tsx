import React, { useState } from 'react'
import { Text, View } from 'react-native'
import QRCodeSvg from 'react-native-qrcode-svg'
import { Metrics, Palette, Typeface } from '../theme/tokens'

/**
 * A QR code for an invoice, an address or an invite. Medium error correction, as on iOS, keeps a
 * long bolt11 at a density a phone can still scan off another phone's screen.
 */
export function QRCode({ text, size }: { text: string; size: number }) {
  // The value that did not fit, so a new value gets its own attempt. Not a flag reset by an
  // effect: the library reports the error while rendering, before a mount effect would run and
  // undo it. For the same reason the state is set once that render is over: setting it during
  // the library's render is an update React warns about.
  const [failed, setFailed] = useState<string>()
  if (failed === text) {
    return (
      <View
        style={{
          width: size,
          height: size,
          borderRadius: Metrics.controlRadius,
          backgroundColor: Palette.subtle,
          alignItems: 'center',
          justifyContent: 'center',
        }}
      >
        <Text style={[Typeface.footnote, { color: Palette.muted }]}>
          Too long for a QR code
        </Text>
      </View>
    )
  }
  return (
    <View accessible accessibilityLabel="QR code">
      <QRCodeSvg
        value={text}
        size={size}
        ecl="M"
        quietZone={0}
        onError={() => Promise.resolve().then(() => setFailed(text))}
      />
    </View>
  )
}
