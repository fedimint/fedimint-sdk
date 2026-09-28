import React, { useState } from 'react'
import { View, Text, TextInput, TouchableOpacity, ScrollView } from 'react-native'
import { useRouter } from 'expo-router'
import { InviteCode, Network, type FederationPreview } from '@fedimint/react-native'
import { errorMessage, open } from '../../src/sdk'
import { useBalance, useWallet } from '../../src/hooks'
import {
  SectionCard,
  SectionTitle,
  Btn,
  Row,
  SuccessBox,
  ErrorBox,
} from '../../src/components'
import s from '../../src/styles'

const TESTNET_FEDERATION_CODE =
  'fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75'

const JoinFederation = ({
  joined,
  refresh,
}: {
  joined: boolean
  refresh: () => void
}) => {
  const [code, setCode] = useState(TESTNET_FEDERATION_CODE)
  const [preview, setPreview] = useState<FederationPreview>()
  const [previewing, setPreviewing] = useState(false)
  const [joinResult, setJoinResult] = useState('')
  const [joinError, setJoinError] = useState('')
  const [joining, setJoining] = useState(false)

  const previewFederation = async () => {
    if (!code.trim()) return
    setPreviewing(true)
    setJoinError('')
    try {
      const session = await open()
      const invite = InviteCode.parse(code.trim())
      setPreview(await session.sdk.preview(invite))
    } catch (error) {
      setJoinError(errorMessage(error))
      setPreview(undefined)
    } finally {
      setPreviewing(false)
    }
  }

  const joinFederation = async () => {
    setJoining(true)
    setJoinError('')
    try {
      const session = await open()
      const invite = InviteCode.parse(code.trim())
      await session.sdk.join(invite)
      setJoinResult('Joined!')
      refresh()
    } catch (error) {
      setJoinError(errorMessage(error))
      setJoinResult('')
    } finally {
      setJoining(false)
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Join Federation</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="Invite Code..."
        placeholderTextColor="#888"
        value={code}
        onChangeText={(text) => {
          setCode(text)
          setPreview(undefined)
        }}
        editable={!joined}
      />
      <Row>
        <Btn
          title={previewing ? 'Previewing...' : 'Preview'}
          onPress={previewFederation}
          disabled={previewing || !code.trim() || joined}
        />
        <Btn
          title={joining ? 'Joining...' : 'Join'}
          onPress={joinFederation}
          disabled={joined || joining}
          primary
        />
      </Row>

      {preview && (
        <View style={s.previewCard}>
          <Text style={s.previewTitle}>Federation Preview</Text>
          <Text style={s.label}>
            Name: <Text style={s.value}>{preview.name ?? 'Unnamed'}</Text>
          </Text>
          <Text style={s.label}>
            Network: <Text style={s.value}>{Network[preview.network]}</Text>
          </Text>
          <Text style={s.label}>
            Guardians: <Text style={s.value}>{preview.guardians}</Text>
          </Text>
          <Text style={s.label}>
            Modules: <Text style={s.value}>{preview.modules.join(', ')}</Text>
          </Text>
        </View>
      )}

      {!joinResult && joined && (
        <Text style={s.italic}>(You've already joined a federation)</Text>
      )}
      {!!joinResult && <SuccessBox>{joinResult}</SuccessBox>}
      {!!joinError && <ErrorBox>{joinError}</ErrorBox>}
    </SectionCard>
  )
}

export default function WalletOverview() {
  const { joined, refresh, version } = useWallet()
  const balance = useBalance(joined, version)
  const router = useRouter()

  return (
    <ScrollView
      style={s.container}
      contentContainerStyle={s.contentContainer}
      keyboardShouldPersistTaps="handled"
    >
      <SectionCard>
        <SectionTitle>Balance</SectionTitle>
        <Text style={s.balanceLarge}>{balance}</Text>
        <Text style={s.balanceLabel}>sats</Text>

        <Row>
          <Text style={s.label}>Federation:</Text>
          <Text style={s.value}>{joined ? 'Joined' : 'Not joined'}</Text>
          <Btn title="Check" onPress={refresh} small />
        </Row>

        <View style={s.actionRow}>
          <TouchableOpacity
            style={s.actionBtn}
            onPress={() => router.push('/send')}
          >
            <Text style={s.actionBtnText}>Send</Text>
          </TouchableOpacity>
          <TouchableOpacity
            style={s.actionBtn}
            onPress={() => router.push('/receive')}
          >
            <Text style={s.actionBtnText}>Receive</Text>
          </TouchableOpacity>
        </View>
      </SectionCard>

      <JoinFederation joined={joined} refresh={refresh} />
    </ScrollView>
  )
}
