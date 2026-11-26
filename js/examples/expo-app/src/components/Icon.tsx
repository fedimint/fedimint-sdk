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
  RefreshCw,
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
 * Every icon the app draws, by its SF Symbol name: drawn as that symbol on iOS, and as the
 * closest Lucide icon on Android, which has no SF Symbols.
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
  'arrow.triangle.2.circlepath': RefreshCw,
  xmark: X,
} satisfies Record<string, LucideIcon>

export type IconName = keyof typeof fallback

/** `size` is the symbol's point size and `weight` its font weight. */
export function Icon({
  name,
  size = 17,
  weight = 'medium',
  color,
  standalone = false,
}: {
  name: IconName
  size?: number
  weight?: SymbolWeight
  color: string
  /** On its own line rather than in a line of text: boxed to the symbol alone. */
  standalone?: boolean
}) {
  if (Platform.OS === 'ios') {
    // Drawn at `size` points in a box a little larger, as a glyph sits in a line of text, so
    // wide symbols are not clipped.
    const box = Math.ceil(size * (standalone ? 1.07 : 1.3))
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
