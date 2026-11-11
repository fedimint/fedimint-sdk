import React, { useCallback, useRef, useState } from 'react'
import { useFocusEffect } from '@react-navigation/native'
import {
  ActivityIndicator,
  Modal,
  Platform,
  Pressable,
  StatusBar,
  StyleSheet,
  Text,
  View,
} from 'react-native'
import { MenuView } from '@react-native-menu/menu'
import { Icon } from '../../components/Icon'
import {
  FederationStatus_Tags,
  type ActivityItem,
} from '@fedimint/react-native'
import { model, useDemo } from '../../model/DemoModel'
import { Button } from '../../components/Buttons'
import { AmountView, Caption, toneColors } from '../../components/Labels'
import { Card, CardList, Screen } from '../../components/Surfaces'
import { ListRow } from '../../components/Rows'
import { ActivityRow } from '../../components/Activity'
import { FederationStatusStyle } from '../../components/Status'
import { FedimintMark } from '../../components/Brand'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

/** How many recent rows Home shows before "See all". */
const recentCount = 3

/** 04 Home: the balance, the two things to do with it, and the latest history. */
export function HomeScreen({
  receive,
  send,
  showActivity,
  showFederations,
  showFederationDetails,
  showRecovery,
  showRecoveryPhrase,
  showOperation,
}: {
  receive: () => void
  send: () => void
  showActivity: () => void
  showFederations: () => void
  showFederationDetails: () => void
  showRecovery: () => void
  showRecoveryPhrase: () => void
  showOperation: (item: ActivityItem) => void
}) {
  const state = useDemo()
  const [menuOpen, setMenuOpen] = useState(false)
  /** Where the menu hangs: under the button that opens it, as iOS's `Menu` does. */
  const [menuTop, setMenuTop] = useState(0)
  const more = useRef<View>(null)
  // The SDK refuses sends and receives until a recovery is done.
  const recovering =
    state.federationStatus?.tag === FederationStatus_Tags.Recovering

  // Once per attach, which empties the list, even one that reattaches the same federation,
  // and again on coming back to a list still empty, as iOS's `.task(id:)` runs on every
  // appearance. Otherwise later loads are a pull or a tap, so a redraw never goes back to the
  // SDK behind the user.
  useFocusEffect(
    useCallback(() => {
      const now = model.getState()
      if (now.hasFederation && now.activityItems.length === 0) model.activity()
    }, [state.attachments]),
  )

  const rails = state.capabilities
    ? [
        state.capabilities.lightning ? 'LN' : undefined,
        state.capabilities.ecash ? 'Ecash' : undefined,
        state.capabilities.onchain ? 'Onchain' : undefined,
      ].filter((rail): rail is string => rail !== undefined)
    : []

  return (
    <View style={{ flex: 1 }}>
      <Screen
        topInset
        onRefresh={() => {
          model.refreshBalance()
          model.activity()
        }}
      >
        <View style={styles.header}>
          <FedimintMark width={27} />
          <Text style={[Typeface.headline, { color: Palette.ink, flex: 1 }]}>
            {state.hasFederation
              ? (state.federationName ?? 'Unnamed federation')
              : 'Fedimint'}
          </Text>
          {Platform.OS === 'ios' ? (
            // The system menu, as the Swift demo's `Menu`: from iOS 26 it opens in Liquid Glass.
            <MenuView
              actions={[
                { id: 'phrase', title: 'Recovery phrase', image: 'key' },
              ]}
              onPressAction={({ nativeEvent }) => {
                if (nativeEvent.event === 'phrase') showRecoveryPhrase()
              }}
            >
              <View accessible accessibilityLabel="More" hitSlop={10}>
                <Icon name="ellipsis.circle" size={20} color={Palette.ink} />
              </View>
            </MenuView>
          ) : (
            <Pressable
              ref={more}
              accessibilityLabel="More"
              hitSlop={10}
              onPress={() =>
                // The menu's modal starts at the top of the screen; Android measures this button
                // from below the status bar.
                more.current?.measureInWindow((_x, y, _width, height) => {
                  const statusBar = StatusBar.currentHeight ?? 0
                  setMenuTop(y + statusBar + height + 8)
                  setMenuOpen(true)
                })
              }
            >
              <Icon name="ellipsis.circle" size={20} color={Palette.ink} />
            </Pressable>
          )}
        </View>

        {state.hasFederation ? (
          <>
            <View style={{ gap: 8 }}>
              <Caption text="Total balance" />
              {state.balanceMsats !== undefined ? (
                <>
                  <AmountView msats={state.balanceMsats} />
                  {/* The amount is the last one that arrived; nothing updates it until a
                      refresh follows the balance again. */}
                  {state.balanceFailure && (
                    <Text
                      selectable
                      style={[Typeface.footnote, { color: Palette.warning }]}
                    >
                      May be out of date: {state.balanceFailure}. Pull down to
                      refresh.
                    </Text>
                  )}
                </>
              ) : state.balanceFailure ? (
                <Text
                  selectable
                  style={[Typeface.footnote, { color: Palette.danger }]}
                >
                  Balance unavailable: {state.balanceFailure}
                </Text>
              ) : (
                <View style={styles.loading}>
                  <Text style={[Typeface.amount, { color: Palette.muted }]}>
                    —
                  </Text>
                  <ActivityIndicator />
                </View>
              )}
              {/* The federation's status and the rails it has, both as the SDK reports them. */}
              <View style={styles.status}>
                <View
                  style={[
                    styles.dot,
                    {
                      backgroundColor:
                        toneColors[
                          FederationStatusStyle.tone(state.federationStatus)
                        ].fg,
                    },
                  ]}
                />
                <Text
                  style={[
                    Typeface.mono('Regular', 13),
                    { color: Palette.muted },
                  ]}
                >
                  {[
                    FederationStatusStyle.label(state.federationStatus),
                    ...rails,
                  ].join(' · ')}
                </Text>
              </View>
            </View>

            {recovering && (
              <Pressable onPress={showRecovery}>
                <Card padding={0}>
                  <ListRow
                    icon="arrow.clockwise"
                    title="Recovering your funds"
                    subtitle="Sending and receiving open once it's done"
                    trailing={
                      <Icon
                        name="chevron.right"
                        size={13}
                        weight="semibold"
                        color={Palette.muted}
                      />
                    }
                  />
                </Card>
              </Pressable>
            )}

            <View style={styles.actions}>
              <Button
                title="Receive"
                icon="arrow.down.left"
                disabled={recovering}
                onPress={receive}
                style={{ flex: 1 }}
              />
              <Button
                title="Send"
                icon="arrow.up.right"
                kind="secondary"
                disabled={recovering}
                onPress={send}
                style={{ flex: 1 }}
              />
            </View>

            <Button
              title="Federation details"
              icon="building.columns"
              kind="secondary"
              onPress={showFederationDetails}
            />

            <View style={{ gap: Metrics.gap }}>
              <View style={styles.recentHeader}>
                <Caption text="Recent activity" />
                <Pressable onPress={showActivity} hitSlop={8}>
                  <Text
                    style={[
                      Typeface.geist('Medium', 15),
                      { color: Palette.primary },
                    ]}
                  >
                    See all
                  </Text>
                </Pressable>
              </View>
              {state.activityItems.length === 0 ? (
                <Card padding={20}>
                  <Text style={[Typeface.callout, { color: Palette.muted }]}>
                    No activity yet. Receive something to get started.
                  </Text>
                </Card>
              ) : (
                <CardList>
                  {state.activityItems.slice(0, recentCount).map((item) => (
                    <Pressable
                      key={item.operationId}
                      onPress={() => showOperation(item)}
                    >
                      <ActivityRow item={item} />
                    </Pressable>
                  ))}
                </CardList>
              )}
            </View>
          </>
        ) : (
          // Before any federation is joined there is nothing to hold a balance in.
          <Card padding={20} style={{ gap: Metrics.gap }}>
            <Text style={[Typeface.headline, { color: Palette.ink }]}>
              Join a federation to start
            </Text>
            <Text style={[Typeface.callout, { color: Palette.muted }]}>
              A federation holds your bitcoin and issues ecash. Paste an invite
              code from its guardians to join.
            </Text>
            <Button
              title="Join a federation"
              onPress={showFederations}
              style={{ marginTop: 4 }}
            />
          </Card>
        )}
      </Screen>

      {/* Android's menu. A modal, so that like iOS's menu it covers the tab bar too: any touch
          outside it, or the back button, closes it. */}
      <Modal
        transparent
        statusBarTranslucent
        animationType="none"
        visible={menuOpen}
        onRequestClose={() => setMenuOpen(false)}
      >
        <Pressable
          style={StyleSheet.absoluteFill}
          onPress={() => setMenuOpen(false)}
        >
          <View style={[styles.menu, { top: menuTop }]}>
            <Pressable
              style={styles.menuItem}
              onPress={() => {
                setMenuOpen(false)
                showRecoveryPhrase()
              }}
            >
              <Text style={[Typeface.body, { color: Palette.ink }]}>
                Recovery phrase
              </Text>
              <Icon name="key" color={Palette.ink} />
            </Pressable>
          </View>
        </Pressable>
      </Modal>
    </View>
  )
}

const styles = StyleSheet.create({
  header: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  loading: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  status: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  dot: { width: 7, height: 7, borderRadius: 3.5 },
  actions: { flexDirection: 'row', gap: Metrics.gap },
  recentHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  menu: {
    position: 'absolute',
    right: Metrics.screenPadding,
    minWidth: 220,
    backgroundColor: Palette.surface,
    borderRadius: 14,
    paddingVertical: 4,
    shadowColor: '#000',
    shadowOpacity: 0.15,
    shadowRadius: 20,
    shadowOffset: { width: 0, height: 8 },
    elevation: 8,
  },
  menuItem: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: 16,
    paddingVertical: 12,
  },
})
