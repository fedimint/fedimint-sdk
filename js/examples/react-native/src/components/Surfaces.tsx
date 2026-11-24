import React, {
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from 'react'
import { NavigationContext } from '@react-navigation/native'
import { BottomTabBarHeightContext } from '@react-navigation/bottom-tabs'
import {
  Keyboard,
  LayoutAnimation,
  Platform,
  RefreshControl,
  ScrollView,
  StatusBar,
  StyleSheet,
  View,
  type ScrollViewProps,
  type ViewStyle,
} from 'react-native'
import { useSafeAreaInsets } from 'react-native-safe-area-context'
import { SafeAreaView } from 'react-native-screens/experimental'
import { Metrics, Palette } from '../theme/tokens'

/** The design's card: a white surface with a hairline border. */
export function Card({
  children,
  padding = 16,
  style,
}: {
  children: React.ReactNode
  padding?: number
  style?: ViewStyle
}) {
  return <View style={[styles.card, { padding }, style]}>{children}</View>
}

/** Rows stacked inside one card with dividers between them, the layout every list uses. */
export function CardList({ children }: { children: React.ReactNode }) {
  const rows = React.Children.toArray(children).filter(Boolean)
  return (
    <Card padding={0}>
      {rows.map((row, index) => (
        <React.Fragment key={index}>
          {index > 0 && <RowDivider inset={16} />}
          {row}
        </React.Fragment>
      ))}
    </Card>
  )
}

/** The hairline between the rows of a card, inset to the rows' text. */
export function RowDivider({ inset = 20 }: { inset?: number }) {
  return <View style={[styles.divider, { marginHorizontal: inset }]} />
}

/**
 * A screen's scrolling body on the design's ground, with its padding and block spacing.
 * `footer` is pinned under it, 16 points above the safe area.
 */
export function Screen({
  children,
  footer,
  gap = Metrics.sectionGap,
  onRefresh,
  topInset = false,
  ...props
}: ScrollViewProps & {
  children: React.ReactNode
  footer?: React.ReactNode
  gap?: number
  /** Pull to refresh. The spinner is the refresh's own; the screen shows what it loads. */
  onRefresh?: () => void
  /** Pads under the status bar, for a screen without a navigation bar. */
  topInset?: boolean
}) {
  const insets = useSafeAreaInsets()
  // Set only inside a tab drawn by the JS tab bar, Android's, which sits over the bottom inset.
  const inTab = useContext(BottomTabBarHeightContext) !== undefined
  const ground = useRef<View>(null)
  const keyboard = useKeyboardOverlap(ground)
  return (
    <View
      ref={ground}
      onLayout={keyboard.onLayout}
      style={[styles.screen, { paddingBottom: keyboard.overlap }]}
    >
      <ScrollView
        style={styles.screen}
        contentContainerStyle={[
          styles.content,
          { gap },
          topInset && { paddingTop: insets.top + Metrics.screenPadding },
        ]}
        refreshControl={
          onRefresh ? (
            <RefreshControl refreshing={false} onRefresh={onRefresh} />
          ) : undefined
        }
        keyboardShouldPersistTaps="handled"
        keyboardDismissMode="interactive"
        {...props}
      >
        {children}
      </ScrollView>
      {footer &&
        (Platform.OS === 'ios' ? (
          // The screen's own safe area, which includes the tab bar; the window's, from
          // `useSafeAreaInsets`, does not. With the keyboard up the screen pads to it instead.
          // `flex: 0`: this view fills the screen by default.
          <SafeAreaView
            edges={{ bottom: keyboard.overlap === 0 }}
            style={{ flex: 0 }}
          >
            <View style={styles.footer}>{footer}</View>
          </SafeAreaView>
        ) : (
          // Inside a tab its bar already covers the inset, and with the keyboard up the screen
          // pads to the keyboard instead, as on iOS.
          <View
            style={[
              styles.footer,
              {
                paddingBottom:
                  (inTab || keyboard.overlap > 0 ? 0 : insets.bottom) +
                  Metrics.screenPadding,
              },
            ]}
          >
            {footer}
          </View>
        ))}
    </View>
  )
}

/**
 * How far the keyboard reaches over the screen, so the screen pads its bottom by that much and
 * keeps its pinned buttons above it. Measured, not assumed: a sheet starts lower than a pushed
 * screen, and an edge-to-edge Android window no longer shrinks for the keyboard. Android measures
 * from below the status bar but places the keyboard from the top of the screen, hence its height.
 *
 * Measured when the keyboard moves while this screen is in front, and when it comes to the front
 * or is laid out with the keyboard already up, such as Review opening from a focused field.
 */
function useKeyboardOverlap(view: React.RefObject<View | null>) {
  const [overlap, setOverlap] = useState(0)
  const navigation = useContext(NavigationContext)
  const measure = useCallback(
    (keyboardTop: number, duration?: number) =>
      view.current?.measureInWindow((_x, y, _width, height) => {
        const top =
          y + (Platform.OS === 'ios' ? 0 : (StatusBar.currentHeight ?? 0))
        if (Platform.OS === 'ios' && duration) {
          LayoutAnimation.configureNext(
            LayoutAnimation.create(duration, 'keyboard', 'opacity'),
          )
        }
        setOverlap(Math.max(top + height - keyboardTop, 0))
      }),
    [view],
  )
  /** Against the keyboard as it stands, for a screen that was not following it. */
  const catchUp = useCallback(() => {
    const keyboard = Keyboard.isVisible() ? Keyboard.metrics() : undefined
    if (keyboard) measure(keyboard.screenY)
    else setOverlap(0)
  }, [measure])

  useEffect(() => {
    const ios = Platform.OS === 'ios'
    const moved = Keyboard.addListener(
      ios ? 'keyboardWillChangeFrame' : 'keyboardDidShow',
      ({ duration, endCoordinates }) => {
        // A screen behind another catches up when it comes back to the front.
        if (navigation && !navigation.isFocused()) return
        measure(endCoordinates.screenY, duration)
      },
    )
    const hidden = Keyboard.addListener(
      ios ? 'keyboardWillHide' : 'keyboardDidHide',
      () => setOverlap(0),
    )
    const focused = navigation?.addListener('focus', catchUp)
    return () => {
      moved.remove()
      hidden.remove()
      focused?.()
    }
  }, [navigation, measure, catchUp])

  return { overlap, onLayout: catchUp }
}

const styles = StyleSheet.create({
  card: {
    backgroundColor: Palette.surface,
    borderRadius: Metrics.cardRadius,
    borderWidth: Metrics.borderWidth,
    borderColor: Palette.border,
    alignSelf: 'stretch',
  },
  divider: { height: Metrics.borderWidth, backgroundColor: Palette.border },
  screen: { flex: 1, backgroundColor: Palette.background },
  content: { padding: Metrics.screenPadding },
  footer: {
    gap: Metrics.gap,
    paddingHorizontal: Metrics.screenPadding,
    paddingTop: Metrics.gap,
    paddingBottom: Metrics.screenPadding,
    backgroundColor: Palette.background,
  },
})
