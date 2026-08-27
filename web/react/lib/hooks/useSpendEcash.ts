import { useCallback, useEffect, useState } from 'react'
import { useFedimintWallet, useOpenWallet } from '.'
import { type SpendNotesState } from '@fedimint/core'

export const useSpendEcash = () => {
  const wallet = useFedimintWallet()
  const { walletStatus } = useOpenWallet()

  const [operationId, setOperationId] = useState<string>()
  const [notes, setNotes] = useState<string>()

  const [state, setState] = useState<SpendNotesState | 'Error'>()
  const [error, setError] = useState<string>()

  useEffect(() => {
    if (!operationId) return

    const unsubscribe = wallet.mint.subscribeSpendNotes(
      operationId,
      (_state) => {
        setState(_state)
      },
      (error) => {
        setState('Error')
        setError(error)
      },
    )

    return () => {
      unsubscribe()
    }
  }, [operationId])

  const spendEcash = useCallback(
    async (amountSats: number, reclaimAfter?: number) => {
      if (walletStatus !== 'open') throw new Error('Wallet is not open')
      const response = await wallet.mint.spendNotes(
        amountSats * 1000,
        reclaimAfter,
      )
      setOperationId(response.operation_id)
      setNotes(response.notes)
      return response.notes
    },
    [wallet, walletStatus],
  )

  return {
    spendEcash,
    notes,
    state,
    error,
  }
}
