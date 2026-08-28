import React, { useState } from 'react'
import {
  View,
  Text,
  TextInput,
  TouchableOpacity,
  ScrollView,
} from 'react-native'
import { useRouter } from 'expo-router'
import { wallet, director } from '../../src/wallet'
import { useIsOpen, useBalance, extractErrorMessage } from '../../src/hooks'
import {
  SectionCard,
  SectionTitle,
  Btn,
  Row,
  SuccessBox,
  ErrorBox,
} from '../../src/components'
import s from '../../src/styles'
import type { PreviewFederation } from '@fedimint/core'

const TESTNET_FEDERATION_CODE =
  'fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75'

const JoinFederation = ({
  open,
  checkIsOpen,
}: {
  open: boolean
  checkIsOpen: () => void | Promise<void>
}) => {
  const [inviteCode, setInviteCode] = useState(TESTNET_FEDERATION_CODE)
  const [previewData, setPreviewData] = useState<PreviewFederation | null>(null)
  const [previewing, setPreviewing] = useState(false)
  const [joinResult, setJoinResult] = useState<string | null>(null)
  const [joinError, setJoinError] = useState('')
  const [joining, setJoining] = useState(false)

  const previewFederationHandler = async () => {
    if (!inviteCode.trim()) return
    setPreviewing(true)
    setJoinError('')
    try {
      const data = await director.previewFederation(inviteCode)
      setPreviewData(data)
    } catch (error) {
      setJoinError(error instanceof Error ? error.message : String(error))
      setPreviewData(null)
    } finally {
      setPreviewing(false)
    }
  }

  const joinFederation = async () => {
    checkIsOpen()
    try {
      if (!wallet) throw new Error('Wallet unavailable')
      setJoining(true)
      await wallet.joinFederation(inviteCode)
      await wallet.open()
      setJoinResult('Joined!')
      setJoinError('')
    } catch (e: any) {
      setJoinError(typeof e === 'object' ? e.toString() : (e as string))
      setJoinResult('')
    } finally {
      setJoining(false)
      checkIsOpen()
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Join Federation</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="Invite Code..."
        placeholderTextColor="#888"
        value={inviteCode}
        onChangeText={(text) => {
          setInviteCode(text)
          setPreviewData(null)
        }}
        editable={!open}
      />
      <Row>
        <Btn
          title={previewing ? 'Previewing...' : 'Preview'}
          onPress={previewFederationHandler}
          disabled={previewing || !inviteCode.trim() || open}
        />
        <Btn
          title={joining ? 'Joining...' : 'Join'}
          onPress={joinFederation}
          disabled={open || joining}
          primary
        />
      </Row>

      {previewData && (
        <View style={s.previewCard}>
          <Text style={s.previewTitle}>Federation Preview</Text>
          <Text style={s.label}>
            Federation ID:{' '}
            <Text style={s.mono}>{previewData.federation_id}</Text>
          </Text>
          <Text style={s.label}>
            Name:{' '}
            <Text style={s.value}>
              {previewData.config.global.meta?.federation_name || 'Unnamed'}
            </Text>
          </Text>
          <Text style={s.label}>
            Consensus Version:{' '}
            <Text style={s.value}>
              {previewData.config.global.consensus_version.major}.
              {previewData.config.global.consensus_version.minor}
            </Text>
          </Text>
          <Text style={s.label}>
            Guardians:{' '}
            <Text style={s.value}>
              {Object.keys(previewData.config.global.api_endpoints).length}
            </Text>
          </Text>

          <Text style={[s.label, { marginTop: 8 }]}>Guardian Endpoints:</Text>
          {Object.entries(previewData.config.global.api_endpoints).map(
            ([id, peer]) => (
              <View key={id} style={s.guardianItem}>
                <Text style={s.guardianName}>{peer.name}</Text>
                <Text style={s.guardianUrl}>{peer.url}</Text>
              </View>
            ),
          )}

          <Text style={[s.label, { marginTop: 8 }]}>Modules:</Text>
          {Object.entries(previewData.config.modules).map(([id, module]) => (
            <Text key={id} style={s.value}>
              • {module.kind}
            </Text>
          ))}
        </View>
      )}

      {!joinResult && open && (
        <Text style={s.italic}>(You've already joined a federation)</Text>
      )}
      {!!joinResult && <SuccessBox>{joinResult}</SuccessBox>}
      {!!joinError && <ErrorBox>{joinError}</ErrorBox>}
    </SectionCard>
  )
}

export default function WalletOverview() {
  const { open, checkIsOpen } = useIsOpen()
  const balance = useBalance(checkIsOpen)
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
        <Text style={s.balanceLabel}>msats</Text>

        <Row>
          <Text style={s.label}>Wallet Status:</Text>
          <Text style={s.value}>{open ? 'Open' : 'Closed'}</Text>
          <Btn title="Check" onPress={checkIsOpen} small />
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

      <JoinFederation open={open} checkIsOpen={checkIsOpen} />
    </ScrollView>
  )
}
