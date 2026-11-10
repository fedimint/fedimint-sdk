import React, { useCallback, useEffect, useRef, useState } from 'react'
import { Share, StyleSheet, Text, View, Pressable } from 'react-native'
import { Icon } from '../../components/Icon'
import {
  EcashReceiveState_Tags,
  EcashSendState,
  LightningRoute_Tags,
  LnReceiveState_Tags,
  LnSendState_Tags,
  OnchainReceiveState_Tags,
  OnchainSendState_Tags,
  type ActivityItem,
  type EcashReceiveState,
  type EcashSendOperationLike,
  type LightningRoute,
  type LnReceiveState,
  type LnSendState,
  type NotesLike,
  type OnchainReceiveState,
  type OnchainSendState,
} from '@fedimint/react-native'
import { model, useCloseOnAttach, useDemo } from '../../model/DemoModel'
import { isAbort, reason } from '../../model/errors'
import { Button } from '../../components/Buttons'
import {
  AmountView,
  Caption,
  SatsFormat,
  StatusPill,
  type Tone,
} from '../../components/Labels'
import { Card, RowDivider, Screen } from '../../components/Surfaces'
import { IconTile, ReviewRow } from '../../components/Rows'
import { ActivityText, ActivityTime } from '../../components/Activity'
import { CopyIconButton, middleTruncated } from '../../components/Pasteboard'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

interface Row {
  label: string
  value: string
  monospaced: boolean
}
interface Facts {
  rows: Row[]
  createdAt: bigint
}
interface Stage {
  label: string
  tone: Tone
  steps: { title: string; reached: boolean }[]
  canCancel?: boolean
}

const step = (title: string, reached: boolean) => ({ title, reached })

/** How a quote names the route: settled inside the federation, or through a gateway. */
export function routeLabel(route: LightningRoute): string {
  return route.tag === LightningRoute_Tags.Internal
    ? 'Internal'
    : `Gateway ${(route.inner as { gatewayId: string }).gatewayId.slice(0, 8)}…`
}

function formatDate(epochMillis: bigint): string {
  return ActivityTime.date(epochMillis).toLocaleString([], {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

// MARK: States to steps, the same ones the iOS demo shows.

function lnSendStage(s: LnSendState): Stage {
  switch (s.tag) {
    case LnSendState_Tags.Created:
      return {
        label: 'Pending',
        tone: 'warning',
        steps: [
          step('Submitted', true),
          step('Funded', false),
          step('Settled', false),
        ],
      }
    case LnSendState_Tags.Funded:
      return {
        label: 'Routing',
        tone: 'warning',
        steps: [
          step('Submitted', true),
          step('Funded', true),
          step('Settled', false),
        ],
      }
    case LnSendState_Tags.Success:
      return {
        label: 'Sent',
        tone: 'success',
        steps: [
          step('Submitted', true),
          step('Funded', true),
          step('Settled', true),
        ],
      }
    case LnSendState_Tags.Refunded:
      return {
        label: 'Refunded',
        tone: 'neutral',
        steps: [
          step('Submitted', true),
          step('Funded', true),
          step('Refunded to your balance', true),
        ],
      }
    default:
      return { label: 'Failed', tone: 'danger', steps: [] }
  }
}

function lnReceiveStage(s: LnReceiveState): Stage {
  switch (s.tag) {
    case LnReceiveState_Tags.Created:
    case LnReceiveState_Tags.WaitingForPayment:
      return {
        label: 'Waiting for payment',
        tone: 'warning',
        steps: [
          step('Invoice created', true),
          step('Paid', false),
          step('Claimed', false),
        ],
      }
    case LnReceiveState_Tags.Funded:
      return {
        label: 'Claiming',
        tone: 'warning',
        steps: [
          step('Invoice created', true),
          step('Paid', true),
          step('Claimed', false),
        ],
      }
    case LnReceiveState_Tags.Claimed:
      return {
        label: 'Received',
        tone: 'success',
        steps: [
          step('Invoice created', true),
          step('Paid', true),
          step('Claimed', true),
        ],
      }
    case LnReceiveState_Tags.Expired:
      return { label: 'Expired', tone: 'neutral', steps: [] }
    case LnReceiveState_Tags.Canceled:
      return { label: 'Canceled', tone: 'neutral', steps: [] }
    default:
      return { label: 'Failed', tone: 'danger', steps: [] }
  }
}

function ecashSendStage(s: EcashSendState): Stage {
  switch (s) {
    case EcashSendState.Created:
      return {
        label: 'Waiting for the recipient',
        tone: 'warning',
        steps: [step('Notes made', true), step('Redeemed', false)],
        canCancel: true,
      }
    case EcashSendState.CancelRequested:
      return {
        label: 'Reclaiming',
        tone: 'warning',
        steps: [step('Notes made', true), step('Reclaimed', false)],
      }
    case EcashSendState.Canceled:
      return {
        label: 'Reclaimed',
        tone: 'neutral',
        steps: [step('Notes made', true), step('Reclaimed', true)],
      }
    case EcashSendState.Redeemed:
      return {
        label: 'Redeemed',
        tone: 'success',
        steps: [step('Notes made', true), step('Redeemed', true)],
      }
  }
}

function ecashReceiveStage(s: EcashReceiveState): Stage {
  switch (s.tag) {
    case EcashReceiveState_Tags.Created:
    case EcashReceiveState_Tags.Issuing:
      return {
        label: 'Redeeming',
        tone: 'warning',
        steps: [step('Submitted', true), step('Issued', false)],
      }
    case EcashReceiveState_Tags.Done:
      return {
        label: 'Received',
        tone: 'success',
        steps: [step('Submitted', true), step('Issued', true)],
      }
    default:
      return { label: 'Failed', tone: 'danger', steps: [] }
  }
}

function onchainSendStage(s: OnchainSendState): Stage {
  switch (s.tag) {
    case OnchainSendState_Tags.Created:
      return {
        label: 'Pending',
        tone: 'warning',
        steps: [step('Submitted', true), step('Broadcast', false)],
      }
    case OnchainSendState_Tags.Succeeded:
      return {
        label: 'Sent',
        tone: 'success',
        steps: [step('Submitted', true), step('Broadcast', true)],
      }
    case OnchainSendState_Tags.Refunded:
      return { label: 'Refunded', tone: 'neutral', steps: [] }
    default:
      return { label: 'Failed', tone: 'danger', steps: [] }
  }
}

function onchainReceiveStage(s: OnchainReceiveState): Stage {
  switch (s.tag) {
    case OnchainReceiveState_Tags.Confirmed:
      return {
        label: 'Claiming',
        tone: 'warning',
        steps: [step('Confirmed', true), step('Claimed', false)],
      }
    case OnchainReceiveState_Tags.Claimed:
      return {
        label: 'Deposited',
        tone: 'success',
        steps: [step('Confirmed', true), step('Claimed', true)],
      }
    default:
      return { label: 'Failed', tone: 'danger', steps: [] }
  }
}

/**
 * Looks one operation up, reads its details, and follows its state while the screen is open.
 * Unlike the Swift bindings, the wait here can be cancelled, so leaving the screen stops it and a
 * later visit follows afresh.
 */
function useOperationFollower(item: ActivityItem) {
  const [facts, setFacts] = useState<Facts>()
  const [stage, setStage] = useState<Stage>()
  const [lost, setLost] = useState<string>()
  const [notes, setNotes] = useState<NotesLike>()
  const [cancelFailure, setCancelFailure] = useState<string>()
  const [cancelling, setCancelling] = useState(false)
  const ecashSend = useRef<EcashSendOperationLike>(undefined)
  const controller = useRef<AbortController>(undefined)

  const start = useCallback(() => {
    controller.current?.abort()
    const current = new AbortController()
    controller.current = current
    const { signal } = current
    setLost(undefined)
    setCancelFailure(undefined)

    /** Pulls states until the final one, which `next()` follows with `undefined`. */
    async function track<S>(
      updates: { next(o: { signal: AbortSignal }): Promise<S | undefined> },
      toStage: (s: S) => Stage,
    ) {
      for (;;) {
        const state = await updates.next({ signal })
        if (signal.aborted || state === undefined) return
        setStage(toStage(state))
        // A state read after a cancel request reflects it, whichever way.
        setCancelling(false)
      }
    }

    ;(async () => {
      try {
        const operation = await model.operation(item.operationId)
        if (signal.aborted) return
        if (!operation) {
          setLost('this federation has no record of the operation')
          return
        }
        const amount = (value: bigint) => SatsFormat.text(value)
        let op
        if ((op = operation.asLnSend())) {
          const d = await op.details({ signal })
          setFacts({
            rows: [
              {
                label: 'Amount',
                value: amount(d.invoiceAmount),
                monospaced: true,
              },
              { label: 'Fee', value: amount(d.fee), monospaced: true },
              { label: 'Total', value: amount(d.total), monospaced: true },
              { label: 'Route', value: routeLabel(d.route), monospaced: true },
            ],
            createdAt: d.createdAt,
          })
          await track(op.updates(), lnSendStage)
        } else if ((op = operation.asLnReceive())) {
          const d = await op.details({ signal })
          setFacts({
            rows: [
              {
                label: 'Invoice amount',
                value: amount(d.invoiceAmount),
                monospaced: true,
              },
              { label: 'Fee', value: amount(d.fee), monospaced: true },
              {
                label: 'Credited',
                value: amount(d.netCredit),
                monospaced: true,
              },
            ],
            createdAt: d.createdAt,
          })
          await track(op.updates(), lnReceiveStage)
        } else if ((op = operation.asEcashSend())) {
          ecashSend.current = op
          const d = await op.details({ signal })
          setNotes(d.notes)
          setFacts({
            rows: [
              {
                label: 'Notes worth',
                value: amount(d.notesValue),
                monospaced: true,
              },
              { label: 'Fee', value: amount(d.fee), monospaced: true },
              {
                label: 'Debited',
                value: amount(d.totalDebited),
                monospaced: true,
              },
              {
                label: 'Reclaimed after',
                value: formatDate(d.reclaimAt),
                monospaced: false,
              },
            ],
            createdAt: d.createdAt,
          })
          await track(op.updates(), ecashSendStage)
        } else if ((op = operation.asEcashReceive())) {
          const d = await op.details({ signal })
          setFacts({
            rows: [
              {
                label: 'Notes worth',
                value: amount(d.notesValue),
                monospaced: true,
              },
              { label: 'Fee', value: amount(d.fee), monospaced: true },
              {
                label: 'Credited',
                value: amount(d.netCredit),
                monospaced: true,
              },
            ],
            createdAt: d.createdAt,
          })
          await track(op.updates(), ecashReceiveStage)
        } else if ((op = operation.asOnchainSend())) {
          const d = await op.details({ signal })
          setFacts({
            rows: [
              {
                label: 'To',
                value: middleTruncated(d.address, 8),
                monospaced: true,
              },
              {
                label: 'Amount',
                value: SatsFormat.sats(d.amount),
                monospaced: true,
              },
              { label: 'Fee', value: amount(d.fee), monospaced: true },
              { label: 'Total', value: amount(d.total), monospaced: true },
            ],
            createdAt: d.createdAt,
          })
          await track(op.updates(), onchainSendStage)
        } else if ((op = operation.asOnchainReceive())) {
          const d = await op.details({ signal })
          const rows: Row[] = [
            {
              label: 'Deposited',
              value: SatsFormat.sats(d.grossDeposited),
              monospaced: true,
            },
            {
              label: 'Transaction',
              value: middleTruncated(d.txid, 8),
              monospaced: true,
            },
          ]
          if (d.fee !== undefined)
            rows.push({ label: 'Fee', value: amount(d.fee), monospaced: true })
          if (d.netCredit !== undefined)
            rows.push({
              label: 'Credited',
              value: amount(d.netCredit),
              monospaced: true,
            })
          setFacts({ rows, createdAt: d.createdAt })
          await track(op.updates(), onchainReceiveStage)
        } else {
          // Recovery, or a kind this build of the SDK does not know.
          setFacts({ rows: [], createdAt: item.time })
        }
      } catch (error) {
        if (!signal.aborted && !isAbort(error)) setLost(reason(error))
      }
    })()
  }, [item])

  useEffect(() => {
    start()
    return () => controller.current?.abort()
  }, [start])

  /** A refused request is its own message, not `lost`: the operation is still followed. */
  const requestCancel = () => {
    const op = ecashSend.current
    if (!op || cancelling) return
    setCancelling(true)
    setCancelFailure(undefined)
    // Left set after a request that went through, until the next state: re-enabling the button
    // before then would invite a second request.
    op.requestCancel().catch((error) => {
      setCancelFailure(reason(error))
      setCancelling(false)
    })
  }

  // Whether the send can still be taken back. Not while the state is lost: it may have been
  // redeemed or reclaimed since the last one read.
  const canCancel = lost === undefined && stage?.canCancel === true
  return {
    facts,
    stage,
    lost,
    notes,
    cancelFailure,
    cancelling,
    canCancel,
    start,
    requestCancel,
  }
}

/**
 * 09 Operation detail: one entry of the history, as the SDK describes it now. The SDK reports an
 * operation's current state and follows it live, but not when each earlier state was reached, so
 * the progress marks the steps reached without times.
 */
export function OperationDetailScreen({
  item,
  close,
}: {
  item: ActivityItem
  close: () => void
}) {
  const state = useDemo()
  const follower = useOperationFollower(item)

  useCloseOnAttach(close)

  const { facts, stage, lost, notes } = follower
  return (
    <Screen
      footer={
        follower.canCancel ? (
          <Button
            title="Request cancel"
            kind="destructive"
            disabled={follower.cancelling}
            onPress={follower.requestCancel}
          />
        ) : undefined
      }
    >
      <View style={styles.header}>
        <IconTile icon={ActivityText.icon(item.kind)} />
        <Text style={[Typeface.footnote, { color: Palette.muted }]}>
          {ActivityText.title(item)}
        </Text>
        {item.amount !== undefined && (
          <AmountView msats={item.amount} sign={ActivityText.sign(item)} />
        )}
        {stage && <StatusPill text={stage.label} tone={stage.tone} center />}
      </View>

      {facts && (
        <Card padding={0}>
          <ReviewRow
            label="Operation ID"
            value={middleTruncated(item.operationId, 6)}
            monospaced
            trailing={<CopyIconButton value={item.operationId} />}
          />
          <RowDivider />
          <ReviewRow
            label="Federation"
            value={state.federationName ?? 'Unnamed'}
          />
          {facts.rows.map((row) => (
            <React.Fragment key={row.label}>
              <RowDivider />
              <ReviewRow
                label={row.label}
                value={row.value}
                monospaced={row.monospaced}
              />
            </React.Fragment>
          ))}
          <RowDivider />
          <ReviewRow label="Created" value={formatDate(facts.createdAt)} />
        </Card>
      )}

      {stage && stage.steps.length > 0 && (
        <View style={{ gap: Metrics.gap }}>
          <Caption text="Progress" />
          <Card style={{ gap: 14 }}>
            {stage.steps.map((s) => (
              <View key={s.title} style={styles.step}>
                <View
                  style={[
                    styles.stepDot,
                    {
                      borderColor: s.reached
                        ? Palette.success
                        : Palette.warning,
                      backgroundColor: s.reached
                        ? Palette.success
                        : 'transparent',
                    },
                  ]}
                />
                <Text
                  style={[
                    Typeface.body,
                    { color: s.reached ? Palette.ink : Palette.warning },
                  ]}
                >
                  {s.title}
                </Text>
              </View>
            ))}
          </Card>
        </View>
      )}

      {/* An unredeemed send's notes, so they can still be handed over. Kept while tracking is
          lost: they may still need handing over. Bearer value, never shown in full. */}
      {stage?.canCancel && notes && (
        <View style={{ gap: Metrics.gap }}>
          <Caption text="Notes" />
          <Card style={styles.notes}>
            <Text
              style={[
                Typeface.mono('Regular', 13),
                { color: Palette.ink, flex: 1 },
              ]}
            >
              {middleTruncated(notes.display(), 10)}
            </Text>
            <CopyIconButton value={notes.display()} bearer />
            <Pressable
              accessibilityLabel="Share"
              hitSlop={6}
              onPress={() => Share.share({ message: notes.display() })}
            >
              <Icon name="square.and.arrow.up" color={Palette.ink} />
            </Pressable>
          </Card>
        </View>
      )}

      {/* Only while a cancel is still possible: once the send has ended either way, a refused
          request no longer matters. */}
      {follower.canCancel && follower.cancelFailure && (
        <Text selectable style={[Typeface.footnote, { color: Palette.danger }]}>
          Couldn't request a cancel: {follower.cancelFailure}
        </Text>
      )}

      {lost && (
        <>
          {/* Reading the state failed; how the operation ended is not known from here. */}
          <Text
            selectable
            style={[Typeface.footnote, { color: Palette.muted }]}
          >
            The SDK stopped reporting this operation's state: {lost}
          </Text>
          <Button title="Try again" kind="secondary" onPress={follower.start} />
        </>
      )}
    </Screen>
  )
}

const styles = StyleSheet.create({
  header: { alignItems: 'center', gap: 10 },
  step: { flexDirection: 'row', alignItems: 'center', gap: 10 },
  stepDot: { width: 10, height: 10, borderRadius: 5, borderWidth: 2 },
  notes: { flexDirection: 'row', alignItems: 'center', gap: 8 },
})
