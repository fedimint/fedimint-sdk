import React from 'react'
import { Text } from 'react-native'
import type { IconName } from './Icon'
import {
  ActivityStatus,
  Direction,
  OperationKind,
  type ActivityItem,
} from '@fedimint/react-native'
import { ListRow } from './Rows'
import { SatsFormat } from './Labels'
import { Palette, Typeface } from '../theme/tokens'

/**
 * One entry of the federation's local history, as Home and Activity show it. Everything is read
 * off the `ActivityItem`; nothing is inferred beyond what the SDK reports. A `refunded` payment
 * reads as such, a state rather than an error, and only a settled credit is coloured, since a
 * pending one in green would read as money that has arrived.
 */
export function ActivityRow({ item }: { item: ActivityItem }) {
  const settledCredit =
    item.direction === Direction.Incoming &&
    item.status === ActivityStatus.Success
  return (
    <ListRow
      icon={ActivityText.icon(item.kind)}
      title={ActivityText.title(item)}
      subtitle={subtitle(item)}
      trailing={
        item.amount !== undefined && (
          <Text
            style={[
              Typeface.figure,
              {
                color: settledCredit
                  ? Palette.success
                  : ActivityText.moved(item)
                    ? Palette.ink
                    : Palette.muted,
              },
            ]}
          >
            {ActivityText.sign(item) + SatsFormat.grouped(item.amount)}
          </Text>
        )
      }
    />
  )
}

function subtitle(item: ActivityItem): string {
  const detail: Record<ActivityStatus, string> = {
    [ActivityStatus.Success]: ActivityTime.short(item.time),
    [ActivityStatus.Pending]: 'Pending',
    [ActivityStatus.Failed]: 'Failed',
    [ActivityStatus.Refunded]: 'Refunded',
    [ActivityStatus.Canceled]: 'Canceled',
    [ActivityStatus.Unknown]: 'Unknown',
  }
  return `${ActivityText.rail(item.kind)} · ${detail[item.status]}`
}

export const ActivityText = {
  /** "Sent", "Receiving", "Deposit": the row's title, also the detail screen's. */
  title(item: ActivityItem): string {
    const s = item.status
    switch (item.kind) {
      case OperationKind.OnchainReceive:
        return 'Deposit'
      case OperationKind.OnchainSend:
        return 'Withdraw'
      case OperationKind.Recovery:
        return s === ActivityStatus.Success
          ? 'Recovered'
          : s === ActivityStatus.Pending
            ? 'Recovering'
            : 'Recovery'
      case OperationKind.Unknown:
        return 'Operation'
      case OperationKind.EcashReceive:
      case OperationKind.LnReceive:
        switch (s) {
          case ActivityStatus.Success:
            return 'Received'
          case ActivityStatus.Pending:
            return 'Receiving'
          // An invoice that expired unpaid ends `canceled`.
          case ActivityStatus.Canceled:
            return 'Not received'
          case ActivityStatus.Failed:
            return 'Receive failed'
          case ActivityStatus.Refunded:
            return 'Refunded'
          default:
            return 'Receive'
        }
      case OperationKind.EcashSend:
      case OperationKind.LnSend:
        switch (s) {
          case ActivityStatus.Success:
            return 'Sent'
          case ActivityStatus.Pending:
            return 'Sending'
          case ActivityStatus.Canceled:
            return 'Canceled'
          case ActivityStatus.Failed:
            return 'Send failed'
          case ActivityStatus.Refunded:
            return 'Refunded'
          default:
            return 'Send'
        }
    }
  },

  /**
   * Whether the amount moved, or is moving: only then is it signed, and only a settled credit is
   * coloured. A failed, refunded or canceled payment moved nothing in the end.
   */
  moved(item: ActivityItem): boolean {
    return (
      item.status === ActivityStatus.Success ||
      item.status === ActivityStatus.Pending
    )
  },

  rail(kind: OperationKind): string {
    switch (kind) {
      case OperationKind.LnSend:
      case OperationKind.LnReceive:
        return 'Lightning'
      case OperationKind.EcashSend:
      case OperationKind.EcashReceive:
        return 'Ecash'
      case OperationKind.OnchainSend:
      case OperationKind.OnchainReceive:
        return 'Onchain'
      default:
        return 'Wallet'
    }
  },

  icon(kind: OperationKind): IconName {
    switch (kind) {
      case OperationKind.LnReceive:
        return 'arrow.down.left'
      case OperationKind.LnSend:
        return 'bolt'
      case OperationKind.EcashSend:
      case OperationKind.EcashReceive:
        return 'banknote'
      case OperationKind.OnchainSend:
      case OperationKind.OnchainReceive:
        return 'link'
      case OperationKind.Recovery:
        return 'arrow.clockwise'
      default:
        return 'questionmark'
    }
  },

  sign(item: ActivityItem): string {
    if (!ActivityText.moved(item)) return ''
    if (item.direction === Direction.Incoming) return '+'
    // U+2212 MINUS SIGN, the width of the plus.
    if (item.direction === Direction.Outgoing) return '−'
    return ''
  },
}

/** How the history renders an SDK `Timestamp`: epoch milliseconds, recorded on this device. */
export const ActivityTime = {
  date(epochMillis: bigint): Date {
    return new Date(Number(epochMillis))
  },
  /** "09:41" today, "Yesterday", otherwise the date. */
  short(epochMillis: bigint): string {
    const date = ActivityTime.date(epochMillis)
    if (sameDay(date, new Date()))
      return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
    if (sameDay(date, yesterday())) return 'Yesterday'
    return date.toLocaleDateString([], {
      day: 'numeric',
      month: 'short',
      year: 'numeric',
    })
  },
  /** "Today", "Yesterday", or the date: the Activity list's day headers. */
  day(date: Date): string {
    if (sameDay(date, new Date())) return 'Today'
    if (sameDay(date, yesterday())) return 'Yesterday'
    return date.toLocaleDateString([], {
      day: 'numeric',
      month: 'short',
      year: 'numeric',
    })
  },
}

function sameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  )
}

function yesterday(): Date {
  const date = new Date()
  date.setDate(date.getDate() - 1)
  return date
}
