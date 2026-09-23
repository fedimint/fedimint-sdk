import React, { useRef, useState } from 'react'
import { Text, TextInput, View, ScrollView } from 'react-native'
import * as Clipboard from 'expo-clipboard'
import {
  Notes,
  EcashSendState,
  LnSendState_Tags,
  OnchainSendState_Tags,
  type EcashQuoteLike,
  type EcashSendOperationLike,
  type LnQuoteLike,
  type OnchainQuoteLike,
} from '@fedimint/react-native'
import { errorMessage, federation, msatToSat, satToMsat } from '../src/sdk'
import {
  SectionCard,
  SectionTitle,
  Btn,
  SuccessBox,
  ErrorBox,
} from '../src/components'
import s from '../src/styles'

const SendLightning = () => {
  const [invoice, setInvoice] = useState('')
  const [quote, setQuote] = useState<LnQuoteLike>()
  const [quoting, setQuoting] = useState(false)
  const [sending, setSending] = useState(false)
  const [result, setResult] = useState('')
  const [error, setError] = useState('')
  // A quote belongs to the input it was requested for: this counts each request so a quote
  // that resolves after the invoice has since changed can tell it is stale and drop itself.
  const quoteGen = useRef(0)

  const handleQuote = async () => {
    const gen = ++quoteGen.current
    setQuoting(true)
    setError('')
    setResult('')
    try {
      const fed = federation()
      if (!fed) throw new Error('Join a federation first')
      const lightning = fed.lightning()
      if (!lightning) {
        throw new Error('Lightning is not supported by this federation')
      }
      const next = await lightning.quote(invoice.trim())
      if (gen !== quoteGen.current) return
      setQuote(next)
    } catch (error) {
      setError(errorMessage(error))
      setQuote(undefined)
    } finally {
      setQuoting(false)
    }
  }

  const handlePay = async () => {
    if (!quote) return
    setSending(true)
    setError('')
    try {
      const lightning = federation()?.lightning()
      if (!lightning) {
        throw new Error('Lightning is not supported by this federation')
      }
      const operation = await lightning.send(quote)
      const state = await operation.awaitFinal()
      setResult(LnSendState_Tags[state.tag])
      setQuote(undefined)
    } catch (error) {
      setError(errorMessage(error))
    } finally {
      setSending(false)
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Pay Lightning</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="lnbc..."
        placeholderTextColor="#888"
        value={invoice}
        onChangeText={(text) => {
          setInvoice(text)
          setQuote(undefined)
          quoteGen.current += 1
        }}
        editable={!quoting}
      />
      <Btn
        title={quoting ? 'Quoting...' : 'Quote'}
        onPress={handleQuote}
        disabled={quoting || !invoice.trim()}
      />

      {quote && (
        <View style={s.resultBox}>
          <Text style={s.label}>
            Fee: <Text style={s.value}>{msatToSat(quote.fee())} sats</Text>
          </Text>
          <Text style={s.label}>
            Total: <Text style={s.value}>{msatToSat(quote.total())} sats</Text>
          </Text>
          <Btn
            title={sending ? 'Paying...' : `Pay ${msatToSat(quote.total())} sats`}
            onPress={handlePay}
            disabled={sending}
            primary
          />
        </View>
      )}
      {!!result && <SuccessBox>{result}</SuccessBox>}
      {!!error && <ErrorBox>{error}</ErrorBox>}
    </SectionCard>
  )
}

const SendOnchain = () => {
  const [address, setAddress] = useState('')
  const [amount, setAmount] = useState('')
  const [quote, setQuote] = useState<OnchainQuoteLike>()
  const [quoting, setQuoting] = useState(false)
  const [sending, setSending] = useState(false)
  const [result, setResult] = useState('')
  const [error, setError] = useState('')
  // A quote belongs to the input it was requested for: this counts each request so a quote
  // that resolves after the amount or address has since changed can tell it is stale and drop
  // itself.
  const quoteGen = useRef(0)

  const handleQuote = async () => {
    const gen = ++quoteGen.current
    setQuote(undefined)
    setResult('')
    setError('')
    setQuoting(true)
    try {
      const fed = federation()
      if (!fed) throw new Error('Join a federation first')
      const onchain = fed.onchain()
      if (!onchain) {
        throw new Error('On-chain is not supported by this federation')
      }
      const next = await onchain.quote(address.trim(), BigInt(amount.trim()))
      if (gen !== quoteGen.current) return
      setQuote(next)
    } catch (error) {
      setError(errorMessage(error))
    } finally {
      setQuoting(false)
    }
  }

  const handleSend = async () => {
    if (!quote) return
    setSending(true)
    setError('')
    try {
      const onchain = federation()?.onchain()
      if (!onchain) {
        throw new Error('On-chain is not supported by this federation')
      }
      const operation = await onchain.send(quote)
      const state = await operation.awaitFinal()
      setResult(OnchainSendState_Tags[state.tag])
      setQuote(undefined)
    } catch (error) {
      setError(errorMessage(error))
    } finally {
      setSending(false)
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Send Onchain</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="Enter amount in sats"
        placeholderTextColor="#888"
        keyboardType="numeric"
        value={amount}
        onChangeText={(text) => {
          setAmount(text)
          setQuote(undefined)
          quoteGen.current += 1
        }}
        editable={!quoting}
      />
      <TextInput
        style={s.input}
        placeholder="Enter onchain address"
        placeholderTextColor="#888"
        value={address}
        onChangeText={(text) => {
          setAddress(text)
          setQuote(undefined)
          quoteGen.current += 1
        }}
        editable={!quoting}
      />
      <Btn
        title={quoting ? 'Quoting...' : 'Quote'}
        onPress={handleQuote}
        disabled={quoting || !amount.trim() || !address.trim()}
      />

      {quote && (
        <View style={s.resultBox}>
          <Text style={s.label}>
            Fee: <Text style={s.value}>{msatToSat(quote.fee())} sats</Text>
          </Text>
          <Text style={s.label}>
            Total: <Text style={s.value}>{msatToSat(quote.total())} sats</Text>
          </Text>
          <Btn
            title={sending ? 'Sending...' : `Send ${msatToSat(quote.total())} sats`}
            onPress={handleSend}
            disabled={sending}
            primary
          />
        </View>
      )}
      {!!result && <SuccessBox>{result}</SuccessBox>}
      {!!error && <ErrorBox>{error}</ErrorBox>}
    </SectionCard>
  )
}

const SendEcash = () => {
  const [amount, setAmount] = useState('')
  const [quote, setQuote] = useState<EcashQuoteLike>()
  const [quoting, setQuoting] = useState(false)
  const [sending, setSending] = useState(false)
  const [notes, setNotes] = useState('')
  const [operation, setOperation] = useState<EcashSendOperationLike>()
  const [status, setStatus] = useState('')
  const [error, setError] = useState('')
  // A quote belongs to the input it was requested for: this counts each request so a quote
  // that resolves after the amount has since changed can tell it is stale and drop itself.
  const quoteGen = useRef(0)

  const handleQuote = async () => {
    const gen = ++quoteGen.current
    setQuoting(true)
    setError('')
    setNotes('')
    setStatus('')
    try {
      const fed = federation()
      if (!fed) throw new Error('Join a federation first')
      const ecash = fed.ecash()
      if (!ecash) throw new Error('Ecash is not supported by this federation')
      const next = await ecash.quote(satToMsat(amount.trim()))
      if (gen !== quoteGen.current) return
      setQuote(next)
    } catch (error) {
      setError(errorMessage(error))
      setQuote(undefined)
    } finally {
      setQuoting(false)
    }
  }

  const handleSend = async () => {
    if (!quote) return
    setSending(true)
    setError('')
    try {
      const ecash = federation()?.ecash()
      if (!ecash) throw new Error('Ecash is not supported by this federation')
      const handle = await ecash.send(quote)
      setNotes(handle.notes.display())
      setOperation(handle.operation)
      setQuote(undefined)
      // The federation never reports a redemption to the sender. The outcome is learnt when
      // the notes are reclaimed: by the deadline in the details, or sooner through Cancel
      // send, which fails against notes the receiver already redeemed and settles the state
      // as Redeemed.
      const details = await handle.operation.details()
      const reclaimAt = new Date(Number(details.reclaimAt)).toLocaleString()
      setStatus(`Outcome settles by ${reclaimAt}, or when you press Cancel send`)
      handle.operation
        .awaitFinal()
        .then((state) => setStatus(EcashSendState[state]))
        .catch((error) => setStatus(errorMessage(error)))
    } catch (error) {
      setError(errorMessage(error))
    } finally {
      setSending(false)
    }
  }

  const handleCancel = async () => {
    if (!operation) return
    setError('')
    try {
      await operation.requestCancel()
      setStatus('Cancel requested; unredeemed notes return to this wallet')
    } catch (error) {
      setError(errorMessage(error))
    }
  }

  const copyNotes = async () => {
    await Clipboard.setStringAsync(notes)
  }


  return (
    <SectionCard>
      <SectionTitle>Send Ecash</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="Amount in sats"
        placeholderTextColor="#888"
        value={amount}
        onChangeText={(text) => {
          setAmount(text)
          setQuote(undefined)
          quoteGen.current += 1
        }}
        keyboardType="numeric"
        editable={!quoting}
      />
      <Btn
        title={quoting ? 'Quoting...' : 'Quote'}
        onPress={handleQuote}
        disabled={quoting || !amount.trim()}
      />

      {quote && (
        <View style={s.resultBox}>
          <Text style={s.label}>
            Notes: <Text style={s.value}>{msatToSat(quote.notesValue())} sats</Text>
          </Text>
          <Text style={s.label}>
            Fee: <Text style={s.value}>{msatToSat(quote.fee())} sats</Text>
          </Text>
          <Text style={s.label}>
            Total: <Text style={s.value}>{msatToSat(quote.total())} sats</Text>
          </Text>
          <Btn
            title={sending ? 'Sending...' : `Send ${msatToSat(quote.total())} sats`}
            onPress={handleSend}
            disabled={sending}
            primary
          />
        </View>
      )}
      {!!notes && (
        <View style={s.invoiceBox}>
          <Text style={s.label}>Notes to hand over:</Text>
          <Text style={s.mono} selectable>
            {notes}
          </Text>
          {!!status && <Text style={s.value}>{status}</Text>}
          <Btn title="Copy" onPress={copyNotes} small />
          <Btn title="Cancel send" onPress={handleCancel} small />
        </View>
      )}
      {!!error && <ErrorBox>{error}</ErrorBox>}
    </SectionCard>
  )
}

const RedeemEcash = () => {
  const [input, setInput] = useState('')
  const [result, setResult] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  const handleRedeem = async () => {
    setBusy(true)
    setResult('')
    setError('')
    try {
      const fed = federation()
      if (!fed) throw new Error('Join a federation first')
      const ecash = fed.ecash()
      if (!ecash) throw new Error('Ecash is not supported by this federation')
      const notes = (() => {
        try {
          return Notes.parse(input.trim())
        } catch (parseError) {
          throw new Error(`Invalid ecash notes: ${errorMessage(parseError)}`)
        }
      })()
      const operation = await ecash.receive(notes)
      await operation.awaitFinal()
      setResult(`Redeemed ${msatToSat(notes.value())} sats`)
    } catch (error) {
      setError(errorMessage(error))
    } finally {
      setBusy(false)
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Redeem Ecash</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="Long ecash string..."
        placeholderTextColor="#888"
        value={input}
        onChangeText={setInput}
      />
      <Btn
        title={busy ? 'Redeeming...' : 'Redeem'}
        onPress={handleRedeem}
        disabled={busy || !input.trim()}
      />
      {!!result && <SuccessBox>{result}</SuccessBox>}
      {!!error && <ErrorBox>{error}</ErrorBox>}
    </SectionCard>
  )
}

export default function SendScreen() {
  return (
    <ScrollView
      style={s.container}
      contentContainerStyle={s.contentContainer}
      keyboardShouldPersistTaps="handled"
    >
      <SendLightning />
      <SendOnchain />
      <RedeemEcash />
      <SendEcash />
    </ScrollView>
  )
}
