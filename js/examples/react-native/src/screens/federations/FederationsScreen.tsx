import React from 'react'
import { ActivityIndicator, Pressable, Text, View } from 'react-native'
import { Icon } from '../../components/Icon'
import { useFocusEffect } from '@react-navigation/native'
import { model, useDemo } from '../../model/DemoModel'
import { Button } from '../../components/Buttons'
import { Caption, SatsFormat } from '../../components/Labels'
import { Card, RowDivider, Screen } from '../../components/Surfaces'
import { FederationRow, networkName } from '../../components/Federation'
import { FederationStatusStyle } from '../../components/Status'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

/**
 * Orders two strings by Unicode code point, as Swift compares them. JavaScript's `<` compares
 * UTF-16 units, which puts a character outside the BMP before U+E000 to U+FFFF.
 */
function byCodePoint(a: string, b: string): number {
  const x = Array.from(a)
  const y = Array.from(b)
  for (let i = 0; i < Math.min(x.length, y.length); i++) {
    const difference = x[i].codePointAt(0)! - y[i].codePointAt(0)!
    if (difference !== 0) return difference
  }
  return x.length - y.length
}

/**
 * 07 Federations: every federation this wallet holds, and the way to join one. The attached one
 * comes first and opens its detail. The others can be switched to, or reconnected if they are
 * not running.
 */
export function FederationsScreen({
  join,
  detail,
}: {
  join: () => void
  detail: () => void
}) {
  const state = useDemo()

  // On focus, which includes coming back from a pushed screen: a failed switch belongs to the
  // visit it happened in.
  useFocusEffect(
    React.useCallback(() => {
      model.clearFailure('switching')
    }, []),
  )

  // Everything but the attached federation, in a stable order: the SDK lists them in none. By
  // name, then id, compared by code point as iOS compares them, not by locale.
  const others = state.storedFederations
    .filter((info) => info.id !== state.federationId)
    .sort(
      (a, b) =>
        byCodePoint(a.name ?? '', b.name ?? '') || byCodePoint(a.id, b.id),
    )
  const switching = state.working.has('switching')
  const attaching = model.isAttaching
  const sending = model.isSending

  return (
    <Screen>
      {state.hasFederation && (
        <View style={{ gap: Metrics.gap }}>
          <Caption text="Active" />
          <Pressable onPress={detail}>
            <Card padding={0}>
              {/* With the failure of a Reconnect started from the detail screen, which can land
                  after the user is back here. */}
              <FederationRow
                name={state.federationName}
                status={state.federationStatus}
                detail={
                  state.balanceMsats !== undefined
                    ? SatsFormat.text(state.balanceMsats)
                    : undefined
                }
                failure={
                  state.switchTarget === state.federationId
                    ? state.failures.switching
                    : undefined
                }
                trailing={
                  <Icon
                    name="chevron.right"
                    size={14}
                    weight="semibold"
                    color={Palette.muted}
                  />
                }
              />
            </Card>
          </Pressable>
        </View>
      )}

      {others.length > 0 && (
        <View style={{ gap: Metrics.gap }}>
          <Caption text={`Other federations · ${others.length}`} />
          <Card padding={0}>
            {others.map((info, index) => {
              const action = FederationStatusStyle.action(info.status)
              return (
                <React.Fragment key={info.id}>
                  {index > 0 && <RowDivider />}
                  <Pressable
                    disabled={action === undefined || attaching || sending}
                    onPress={() => model.switchFederation(info.id)}
                  >
                    <FederationRow
                      name={info.name}
                      status={info.status}
                      detail={networkName(info.network)}
                      failure={
                        state.switchTarget === info.id
                          ? state.failures.switching
                          : undefined
                      }
                      trailing={
                        switching && state.switchTarget === info.id ? (
                          <ActivityIndicator />
                        ) : action ? (
                          <Text
                            style={[Typeface.callout, { color: Palette.navy }]}
                          >
                            {action}
                          </Text>
                        ) : undefined
                      }
                    />
                  </Pressable>
                </React.Fragment>
              )
            })}
          </Card>
        </View>
      )}

      {!state.hasFederation && others.length === 0 && (
        <Card padding={20}>
          <Text style={[Typeface.callout, { color: Palette.muted }]}>
            You haven't joined a federation yet. Join one with an invite code
            from its guardians.
          </Text>
        </Card>
      )}

      <Button
        title="Join with invite code"
        icon="plus"
        kind="secondary"
        onPress={join}
      />
    </Screen>
  )
}
