import { Platform, Pressable, Text } from 'react-native'
import { DefaultTheme } from '@react-navigation/native'
import type { ActivityItem } from '@fedimint/react-native'
import { ActivityText } from '../components/Activity'
import { Icon } from '../components/Icon'
import { Palette, Typeface } from '../theme/tokens'

// What the routes in app/ share: the bars' look, the sheets' Close button, and the operation a
// pushed detail screen shows.

export const theme = {
  ...DefaultTheme,
  colors: {
    ...DefaultTheme.colors,
    primary: Palette.ink,
    background: Palette.background,
    card: Palette.background,
    text: Palette.ink,
    border: Palette.border,
  },
}

/** The design's bars: cream, an inline title, no hairline. */
export const stackOptions = {
  headerStyle: { backgroundColor: Palette.background },
  headerShadowVisible: false,
  headerTitleStyle: {
    fontFamily: 'Geist-SemiBold',
    fontSize: 17,
    color: Palette.ink,
  },
  headerTintColor: Palette.ink,
  headerBackButtonDisplayMode: 'minimal' as const,
  // Centred on Android too, whose default puts it beside the back button.
  headerTitleAlign: 'center' as const,
  contentStyle: { backgroundColor: Palette.background },
}

/** A tab's stack: as `stackOptions`, with the bar's controls in the primary colour. */
export const tabStackOptions = {
  ...stackOptions,
  headerTintColor: Palette.primary,
}

/**
 * "Close" on a sheet's leading edge: a native bar button on iOS, the glass capsule from iOS 26;
 * Android has no such item and draws the text.
 */
export function closeButton(close: () => void) {
  return Platform.OS === 'ios'
    ? {
        unstable_headerLeftItems: () => [
          { type: 'button' as const, label: 'Close', onPress: close },
        ],
      }
    : {
        headerLeft: () => (
          <Pressable onPress={close} hitSlop={8}>
            <Text style={[Typeface.body, { color: Palette.ink }]}>Close</Text>
          </Pressable>
        ),
      }
}

/** `closeButton`'s place left empty. */
export const noCloseButton =
  Platform.OS === 'ios'
    ? { unstable_headerLeftItems: () => [] }
    : { headerLeft: () => null }

/** The "+" that opens Join, in the Federations bar. */
export function joinButton(join: () => void) {
  return Platform.OS === 'ios'
    ? {
        unstable_headerRightItems: () => [
          {
            type: 'button' as const,
            label: 'Join a federation',
            icon: { type: 'sfSymbol' as const, name: 'plus' as const },
            // Its own tint: from iOS 26 the glass item does not take the bar's.
            tintColor: Palette.primary,
            onPress: join,
          },
        ],
      }
    : {
        headerRight: () => (
          <Pressable
            accessibilityLabel="Join a federation"
            hitSlop={8}
            onPress={join}
          >
            <Icon name="plus" size={20} color={Palette.primary} />
          </Pressable>
        ),
      }
}

/**
 * The operations detail screens show, by ID: route params are URL strings and an item carries
 * BigInts, so the route gets the ID and reads the item here. Dropped when the last screen showing
 * it closes, since Home and Activity can each have the same one open.
 */
const operations = new Map<string, ActivityItem>()
/** How many detail screens show each operation. */
const operationScreens = new Map<string, number>()

/** For as long as a detail screen shows the operation; returns what to call when it closes. */
export function holdOperation(id: string): () => void {
  operationScreens.set(id, (operationScreens.get(id) ?? 0) + 1)
  return () => {
    const left = (operationScreens.get(id) ?? 1) - 1
    if (left > 0) {
      operationScreens.set(id, left)
    } else {
      operationScreens.delete(id)
      operations.delete(id)
    }
  }
}

/** Opens `item`'s detail in `tab`, storing it only if the push happened: none other frees it. */
export function openOperation(
  push: (href: {
    pathname: `/${'home' | 'activity'}/operation`
    params: { id: string }
  }) => boolean,
  tab: 'home' | 'activity',
  item: ActivityItem,
) {
  const pushed = push({
    pathname: `/${tab}/operation`,
    params: { id: item.operationId },
  })
  if (pushed) operations.set(item.operationId, item)
}

export function openedOperation(id: string | undefined) {
  return id === undefined ? undefined : operations.get(id)
}

/** An operation's screen is titled with its rail. */
export function operationTitle({ route }: { route: { params?: object } }) {
  const id = (route.params as { id?: string } | undefined)?.id
  const item = openedOperation(id)
  return { title: item ? ActivityText.rail(item.kind) : '' }
}
