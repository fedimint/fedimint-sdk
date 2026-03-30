import React from 'react'
import { ScrollView, Text } from 'react-native'
import s from '../styles'
import { useIsOpen, useBalance } from '../hooks'
import { SectionCard, SectionTitle, Row, Btn } from '../components/common'

export const WalletScreen = () => {
  const { open, checkIsOpen } = useIsOpen()
  const balance = useBalance(checkIsOpen)

  return (
    <ScrollView style={s.container} contentContainerStyle={s.contentContainer}>
      <Text style={s.header}>Overview</Text>
      <SectionCard>
        <SectionTitle>Wallet Status</SectionTitle>
        <Row>
          <Text style={s.label}>Is Wallet Open?</Text>
          <Text style={s.value}>{open ? 'Yes' : 'No'}</Text>
          <Btn title="Check" onPress={checkIsOpen} small />
        </Row>
        <Row>
          <Text style={s.label}>Balance:</Text>
          <Text style={s.balance}>{balance}</Text>
          <Text style={s.value}> sats</Text>
        </Row>
      </SectionCard>
    </ScrollView>
  )
}
