import { useCallback, useEffect, useState } from 'react'
import { useFedimintWallet, useOpenWallet } from '.'
import { LnReceiveState, type CreateBolt11Response } from '@fedimint/core'

export const useReceiveLightning = () => {
  const wallet = useFedimintWallet()
  const { walletStatus } = useOpenWallet()
  const [invoice, setInvoice] = useState<CreateBolt11Response>()
  const [invoiceReceiveState, setInvoiceReceiveState] =
    useState<LnReceiveState>()
  const [error, setError] = useState<string>()

  const generateInvoice = useCallback(
    async (amount: number, description: string) => {
      if (walletStatus !== 'open') throw new Error('Wallet is not open')
      const response = await wallet.lightning.createInvoice(amount, description)
      setInvoice(response)
      return response.invoice
    },
    [wallet, walletStatus],
  )

  useEffect(() => {
    if (walletStatus !== 'open' || !invoice) return
    const unsubscribe = wallet.lightning.subscribeLnReceive(
      invoice.operation_id,
      (state) => {
        setInvoiceReceiveState(state)
      },
      (error) => {
        setError(error)
      },
    )

    return () => {
      unsubscribe()
    }
  }, [walletStatus, invoice])

  return {
    generateInvoice,
    bolt11: invoice?.invoice,
    invoiceStatus: invoiceReceiveState,
    error,
  }
}
