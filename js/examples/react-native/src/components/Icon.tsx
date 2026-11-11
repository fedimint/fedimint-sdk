import React from 'react'
import { Platform } from 'react-native'
import { SFSymbol, type SymbolWeight } from 'react-native-sfsymbols'
import {
  Activity,
  ArrowDownLeft,
  ArrowUpRight,
  Banknote,
  Check,
  ChevronRight,
  CircleEllipsis,
  CircleHelp,
  ClipboardPaste,
  Copy,
  House,
  Key,
  Landmark,
  Link,
  Lock,
  Plus,
  RotateCw,
  Share,
  ShieldCheck,
  TriangleAlert,
  Undo2,
  X,
  Zap,
  type LucideIcon,
} from 'lucide-react-native'

/**
 * Every icon the demo draws, by its SF Symbol name: the one the iOS demo uses. On iOS it is
 * drawn as that symbol, so the two demos match exactly; Android has no SF Symbols and draws the
 * closest Lucide icon instead.
 */
const fallback = {
  house: House,
  'waveform.path.ecg': Activity,
  'building.columns': Landmark,
  'arrow.down.left': ArrowDownLeft,
  'arrow.up.right': ArrowUpRight,
  bolt: Zap,
  banknote: Banknote,
  link: Link,
  'arrow.clockwise': RotateCw,
  questionmark: CircleHelp,
  'doc.on.doc': Copy,
  checkmark: Check,
  'square.and.arrow.up': Share,
  'doc.on.clipboard': ClipboardPaste,
  lock: Lock,
  'checkmark.shield': ShieldCheck,
  key: Key,
  'ellipsis.circle': CircleEllipsis,
  'chevron.right': ChevronRight,
  plus: Plus,
  'exclamationmark.triangle': TriangleAlert,
  'arrow.uturn.backward': Undo2,
  xmark: X,
} satisfies Record<string, LucideIcon>

export type IconName = keyof typeof fallback

/**
 * `size` is the point size the iOS demo sets with `.font(.system(size:weight:))`, or the text
 * style's size for an icon inside a label; `weight` is that font's weight.
 */
export function Icon({
  name,
  size = 17,
  weight = 'medium',
  color,
}: {
  name: IconName
  size?: number
  weight?: SymbolWeight
  color: string
}) {
  if (Platform.OS === 'ios') {
    // The symbol is drawn at `size` points inside a box a little larger, as a glyph sits in a
    // line of text, so wide symbols are not clipped.
    const box = Math.ceil(size * 1.3)
    return (
      <SFSymbol
        name={name}
        size={size}
        weight={weight}
        color={color}
        resizeMode="center"
        style={{ width: box, height: box }}
      />
    )
  }
  const Fallback = fallback[name]
  return (
    <Fallback
      size={size}
      color={color}
      strokeWidth={weight === 'semibold' || weight === 'bold' ? 2.4 : 2}
    />
  )
}
