import type { TextStyle } from 'react-native'

// The unified demo design's tokens: the one place a colour, typeface, size or spacing is
// decided. The same design drives the iOS, Android and web demos, so a value changed here
// should change there too (ios/Demo/Views/Theme/Tokens.swift is the iOS copy).

export const Palette = {
  /** Titles and body text. */
  ink: '#0B0B1F',
  /** The darkest surface: the welcome screen, avatars, a selected chip. */
  navy: '#0A0A2A',
  /** Primary buttons, links, the active tab. */
  primary: '#181884',
  /** The brand highlight, from the Fedimint mark. */
  accent: '#4AD6FE',
  /** Secondary text, labels, inactive tabs. */
  muted: '#65657A',

  /** Every screen's ground. */
  background: '#F6F6F2',
  /** Cards, inputs, secondary buttons. */
  surface: '#FFFFFF',
  border: '#E4E4DC',
  /** Icon tiles and the neutral pill. */
  subtle: '#EEEEE8',

  // Status. Each tone is a foreground with the tint it sits on in a pill.
  success: '#0E7A4B',
  successTint: '#DDF1E6',
  warning: '#9A5800',
  warningTint: '#FAEED7',
  danger: '#C0302B',
  dangerTint: '#FAE3E1',
} as const

type Weight = 'Regular' | 'Medium' | 'SemiBold' | 'Bold'

// Geist for text and Geist Mono for figures and labels, both bundled (assets/fonts). The
// family is the font's PostScript name, which iOS resolves, and its file name, which Android
// does; the two are the same for every bundled file.
const geist = (weight: Weight, fontSize: number): TextStyle => ({
  fontFamily: `Geist-${weight}`,
  fontSize,
})
const mono = (
  weight: Exclude<Weight, 'Bold'>,
  fontSize: number,
): TextStyle => ({
  fontFamily: `GeistMono-${weight}`,
  fontSize,
})

export const Typeface = {
  /** Screen titles: "Write down your recovery phrase", "Federations". */
  title: geist('Bold', 30),
  /** Navigation titles and row titles. */
  headline: geist('SemiBold', 17),
  body: geist('Regular', 16),
  /** Explanatory copy under a title. */
  callout: geist('Regular', 15),
  footnote: geist('Regular', 13),
  /** Button labels. */
  button: geist('Medium', 17),

  /** The small uppercase labels: "STEP 1 OF 2", "TOTAL BALANCE". */
  label: {
    ...mono('Medium', 11),
    letterSpacing: 0.8,
    textTransform: 'uppercase',
  } as TextStyle,
  /** Invoices, addresses, ids and seed words. */
  code: mono('Regular', 15),

  /** The headline figure on Home and the send and receive screens. */
  amount: mono('SemiBold', 44),
  /** The unit beside it. */
  amountUnit: geist('SemiBold', 20),
  /** A figure inside a row or a quote summary. */
  figure: mono('Medium', 15),

  geist,
  mono,
}

export const Metrics = {
  /** Side padding on every screen. */
  screenPadding: 20,
  /** Between related elements: two buttons side by side, a label and its field. */
  gap: 12,
  /** Between the blocks of a screen. */
  sectionGap: 24,

  buttonHeight: 52,
  rowHeight: 68,
  seedWordHeight: 46,

  cardRadius: 16,
  controlRadius: 14,
  borderWidth: 1,
} as const
